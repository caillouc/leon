//! Right pane (CTX-1, GIT-*, GH-1): uncommitted changes, branch diff, pull
//! request and information about the flow, plus the pending review.

use std::collections::HashMap;

use gpui::{
    AnyElement, Context, Entity, FontWeight, IntoElement, SharedString, Subscription, Window, div,
    prelude::*,
};
use gpui_component::input::{InputEvent, TextareaState};
use leon_core::Intent;
use leon_domain::{ConversationId, FlowId, ReviewComment, review_message};

use crate::diff_drawer::Drawer;
use crate::diff_rows::{DiffList, counts};
use crate::diff_view::{DiffSource, DraftAt};
use crate::icons::{Icon, icon};

use crate::theme;
use crate::widgets::{HEADER_HEIGHT, button, empty, primary};
use crate::workspace::{Screen, Workspace};

/// The right pane's tabs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RightTab {
    Info,
    Changes,
    Branch,
}

/// A comment in the pending review, and the diff it was written on.
#[derive(Debug, Clone)]
pub(crate) struct Pending {
    pub source: DiffSource,
    /// The folder of that diff (CTX-2).
    pub folder: Option<std::path::PathBuf>,
    pub comment: ReviewComment,
}

/// What the right pane remembers between renders.
pub(crate) struct RightState {
    /// The tab picked by hand, per flow; else Info.
    pub tabs: HashMap<FlowId, RightTab>,
    /// The folder picked by hand, per flow working in several and per tab
    /// (the Flow's, or an agent's) (CTX-2).
    pub folder: HashMap<(FlowId, Option<ConversationId>), std::path::PathBuf>,
    /// Syntax colours of the files diffs show, by content (I21).
    pub syntax: HashMap<u64, crate::diff_rows::FileStyles>,
    /// Files the user opened or closed by hand, overriding `diff.fold`.
    pub open: HashMap<(FlowId, DiffSource, String), bool>,
    /// Pending review per flow (GIT-5).
    pub review: HashMap<FlowId, Vec<Pending>>,
    /// Conversation the review goes to, when the user picked one.
    pub review_to: HashMap<FlowId, ConversationId>,
    /// The line a comment is being written on.
    pub draft: Option<DraftAt>,
    /// Lines being chosen by dragging a line's +, before the comment box.
    pub selecting: Option<crate::diff_rows::Selecting>,
    /// The drawer shows diffs side by side instead of unified.
    pub split: bool,
    /// Side by side: the old side's share of the width, and whether its
    /// divider is being dragged (GIT-2).
    pub split_ratio: f32,
    pub split_drag: bool,
    pub split_area: crate::widgets::Area,
    pub comment: Entity<TextareaState>,
    /// The answer to a review thread being written, and that thread's id
    /// (GH-8).
    pub thread_reply: Entity<TextareaState>,
    pub replying: Option<String>,
    pub lists: HashMap<(FlowId, DiffSource), DiffList>,
    /// The diff drawer, when open.
    pub drawer: Option<Drawer>,
}

impl RightState {
    pub(crate) fn new(
        window: &mut Window,
        cx: &mut Context<Workspace>,
    ) -> (Self, Vec<Subscription>) {
        let comment = cx.new(|cx| {
            TextareaState::new(window, cx)
                .auto_grow(2, 8)
                .submit_on_enter(true)
                .placeholder("Comment for the agent")
        });
        let thread_reply = cx.new(|cx| {
            TextareaState::new(window, cx)
                .auto_grow(2, 8)
                .placeholder("Reply to the thread")
        });
        let subscriptions = vec![cx.subscribe_in(
            &comment,
            window,
            |this, _, event: &InputEvent, window, cx| {
                if let InputEvent::PressEnter { shift: false, .. } = event {
                    this.save_comment(window, cx);
                }
            },
        )];
        let state = Self {
            tabs: HashMap::new(),
            folder: HashMap::new(),
            syntax: HashMap::new(),
            open: HashMap::new(),
            review: HashMap::new(),
            review_to: HashMap::new(),
            draft: None,
            selecting: None,
            split: false,
            split_ratio: 0.5,
            split_drag: false,
            split_area: std::rc::Rc::default(),
            comment,
            thread_reply,
            replying: None,
            lists: HashMap::new(),
            drawer: None,
        };
        (state, subscriptions)
    }
}

impl Workspace {
    pub(crate) fn render_right(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let frame = div()
            .w(crate::theme::rem(self.panes.right))
            .h_full()
            .flex_none()
            .flex()
            .flex_col()
            .bg(theme::panel())
            .border_l_1()
            .border_color(theme::border());
        let Screen::Flow(id) = self.screen else {
            return frame
                .child(empty(
                    "Select a flow to see its changes, branch diff and pull request.",
                ))
                .into_any_element();
        };
        let body = match self.right_tab(id) {
            RightTab::Changes => self.render_diff(id, DiffSource::Changes, cx),
            RightTab::Branch => self.render_diff(id, DiffSource::Branch, cx),
            RightTab::Info => self.render_info(id, cx),
        };
        frame
            .children(self.right_header(id, cx))
            .child(div().flex_1().min_h_0().flex().flex_col().child(body))
            // While the drawer is open, the review is sent from there.
            .when(self.right.drawer.is_none(), |d| {
                d.children(self.render_review_bar(id, window, cx))
            })
            .into_any_element()
    }

    /// The tab shown for flow `id`.
    pub(crate) fn right_tab(&self, id: FlowId) -> RightTab {
        self.right.tabs.get(&id).copied().unwrap_or(RightTab::Info)
    }

    /// The pane's top: its tabs at the height of the flow's header, then on
    /// Changes and Branch the repository and worktree they show (CTX-1).
    fn right_header(&self, id: FlowId, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let row = |height: f32| {
            div()
                .flex_none()
                .h(crate::theme::rem(height))
                .px_2()
                .flex()
                .border_b_1()
                .border_color(theme::border())
        };
        let tabs = self.right_tabs(id, cx);
        // Closes the pane; the flow's header opens it again.
        let close = div()
            .id("right-close")
            .flex_none()
            .p(crate::theme::rem(4.))
            .rounded_md()
            .cursor_pointer()
            .hover(|s| s.bg(theme::surface_2()))
            .child(icon(Icon::Panel, theme::fg_muted(), 15.))
            .on_click(cx.listener(|this, _, _, cx| {
                this.panes.right_open = false;
                cx.notify();
            }));
        let tabs_row = row(HEADER_HEIGHT)
            .items_end()
            .child(tabs)
            .child(div().flex_1())
            .child(div().mb(crate::theme::rem(8.)).child(close))
            .into_any_element();
        // Changes and Branch show one worktree: which, under the tabs. Info
        // is the whole flow's.
        let bar = match self.right_tab(id) {
            RightTab::Changes | RightTab::Branch => self.repo_bar(id, cx),
            RightTab::Info => None,
        };
        std::iter::once(tabs_row)
            .chain(bar.map(|bar| row(36.).items_center().px_3().child(bar).into_any_element()))
            .collect()
    }

    /// The tabs: Info, Changes, Branch, the last two with their lines.
    fn right_tabs(&self, id: FlowId, cx: &mut Context<Self>) -> AnyElement {
        let git = self.folder_git(id);
        // Lines added and removed, as git counts them.
        let lines = |files: &[leon_domain::FileDiff]| {
            (!files.is_empty()).then(|| {
                let added: u32 = files.iter().map(|f| f.added).sum();
                let removed: u32 = files.iter().map(|f| f.removed).sum();
                counts(added, removed)
            })
        };
        let tabs = [
            (RightTab::Info, "Info", None),
            (
                RightTab::Changes,
                "Changes",
                git.and_then(|g| lines(&g.changes)),
            ),
            (
                RightTab::Branch,
                "Branch",
                git.and_then(|g| lines(&g.branch_diff)),
            ),
        ];
        div()
            .flex()
            .items_end()
            .gap(crate::theme::rem(2.))
            .children(tabs.into_iter().map(|(tab, text, n)| {
                let selected = self.right_tab(id) == tab;
                div()
                    .id(SharedString::from(format!("right-tab-{text}")))
                    .px_2()
                    .pb(crate::theme::rem(10.))
                    .border_b_2()
                    .border_color(if selected {
                        theme::accent()
                    } else {
                        gpui::transparent_black().into()
                    })
                    .text_size(crate::theme::rem(12.5))
                    .text_color(if selected {
                        theme::fg()
                    } else {
                        theme::fg_muted()
                    })
                    .cursor_pointer()
                    .hover(|s| s.text_color(theme::fg()))
                    .flex()
                    .gap(crate::theme::rem(5.))
                    .child(text)
                    .items_center()
                    .children(n.map(|n| div().text_size(crate::theme::rem(11.)).child(n)))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.right.tabs.insert(id, tab);
                        match tab {
                            RightTab::Changes | RightTab::Branch => {
                                this.core.send(Intent::RefreshGit(id));
                            }
                            RightTab::Info => {}
                        }
                        cx.notify();
                    }))
            }))
            .into_any_element()
    }

    /// The Info tab (CTX-1), for the whole flow, whatever the repository or
    /// worktree picked for Changes and Branch: its pull requests, its
    /// issues, then its to-dos.
    fn render_info(&self, id: FlowId, cx: &mut Context<Self>) -> AnyElement {
        let Some(view) = self.snapshot.flow(id) else {
            return empty("This flow no longer exists.");
        };
        div()
            .id("right-info")
            .flex_1()
            .overflow_y_scroll()
            .py_3()
            .flex()
            .flex_col()
            // Its pull requests and issues, then its to-dos (TODO-2).
            .children(self.info_refs(view, cx))
            .child(self.todo_section(id, cx))
            .into_any_element()
    }

    pub(crate) fn render_review_bar(
        &self,
        id: FlowId,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let count = self.right.review.get(&id).map_or(0, Vec::len);
        if count == 0 {
            return None;
        }
        let view = self.snapshot.flow(id)?;
        let target = self.review_target(id)?;
        let choices = self
            .snapshot
            .conversations_of(view)
            .into_iter()
            .map(|s| (s.conversation.id.0.to_string(), s.name.clone()))
            .collect();
        let current = self
            .snapshot
            .conversations
            .get(&target)
            .map_or_default(|s| s.name.clone());
        let to = self.picker(
            "review-to",
            &current,
            choices,
            cx,
            move |this, value, _, _| {
                if let Ok(raw) = value.parse() {
                    this.right.review_to.insert(id, ConversationId(raw));
                }
            },
        );
        Some(
            div()
                .flex()
                .flex_col()
                .flex_none()
                .gap_2()
                .px_3()
                .py(crate::theme::rem(9.))
                .border_t_1()
                .border_color(theme::border())
                .bg(theme::surface())
                .text_size(crate::theme::rem(12.))
                .child(
                    div()
                        .flex()
                        .gap_2()
                        .child(
                            div()
                                .font_weight(FontWeight::SEMIBOLD)
                                .child("Pending review"),
                        )
                        .child(div().text_color(theme::fg_muted()).child(format!(
                            "{count} comment{}",
                            if count == 1 { "" } else { "s" }
                        ))),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(div().text_color(theme::fg_muted()).child("Send to"))
                        .child(to)
                        .child(div().flex_1())
                        .child(button(
                            "review-discard",
                            "Discard",
                            cx.listener(move |this, _, _, cx| {
                                this.right.review.remove(&id);
                                cx.notify();
                            }),
                        ))
                        .child(primary(
                            "review-send",
                            "Send review",
                            cx.listener(move |this, _, _, cx| this.send_review(id, cx)),
                        )),
                )
                .into_any_element(),
        )
    }

    /// The conversation a review goes to: the one picked, else the one open in
    /// the main pane, else the first.
    fn review_target(&self, id: FlowId) -> Option<ConversationId> {
        let conversations = &self.snapshot.flow(id)?.conversations;
        self.right
            .review_to
            .get(&id)
            .copied()
            .filter(|s| conversations.contains(s))
            .or_else(|| self.current_conversation())
            .or_else(|| conversations.first().copied())
    }

    fn send_review(&mut self, id: FlowId, cx: &mut Context<Self>) {
        let Some(conversation) = self.review_target(id) else {
            return;
        };
        let Some(pending) = self.right.review.remove(&id) else {
            return;
        };
        // With several folders, each file by its full path (CTX-2).
        let several = self.snapshot.flow(id).is_some_and(|c| c.folders.len() > 1);
        let comments: Vec<ReviewComment> = pending
            .into_iter()
            .map(|p| match (several, &p.folder) {
                (true, Some(folder)) => ReviewComment {
                    path: folder.join(&p.comment.path).display().to_string(),
                    ..p.comment
                },
                _ => p.comment,
            })
            .collect();
        self.core.send(Intent::Send {
            conversation,
            text: review_message(&comments),
            images: Vec::new(),
        });
        cx.notify();
    }
}
