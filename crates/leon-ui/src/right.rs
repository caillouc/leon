//! Right pane (CTX-1, GIT-*, GH-1): uncommitted changes, branch diff, pull
//! request and information about the conversation, plus the pending review.

use std::collections::HashMap;

use gpui::{
    AnyElement, Context, Entity, FontWeight, IntoElement, SharedString, Subscription, Window, div,
    prelude::*, px,
};
use gpui_component::input::{InputEvent, TextareaState};
use leon_core::Intent;
use leon_domain::{AgentSessionId, ConversationId, ReviewComment, review_message};

use crate::diff_view::{DiffList, DiffSource, DraftAt};
use crate::theme;
use crate::widgets::{button, empty, label, primary};
use crate::workspace::{Screen, Workspace};

const WIDTH: f32 = 440.;

/// The right pane's tabs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RightTab {
    Changes,
    Branch,
    Pr,
    Info,
}

/// A comment in the pending review, and the diff it was written on.
#[derive(Debug, Clone)]
pub(crate) struct Pending {
    pub source: DiffSource,
    pub comment: ReviewComment,
}

/// What the right pane remembers between renders.
pub(crate) struct RightState {
    pub tab: RightTab,
    /// Files the user opened or closed by hand, overriding `diff.fold`.
    pub open: HashMap<(ConversationId, DiffSource, String), bool>,
    /// Pending review per conversation (GIT-5).
    pub review: HashMap<ConversationId, Vec<Pending>>,
    /// Session the review goes to, when the user picked one.
    pub review_to: HashMap<ConversationId, AgentSessionId>,
    /// The line a comment is being written on.
    pub draft: Option<DraftAt>,
    pub comment: Entity<TextareaState>,
    pub pr_comment: Entity<TextareaState>,
    pub lists: HashMap<(ConversationId, DiffSource), DiffList>,
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
        let pr_comment = cx.new(|cx| {
            TextareaState::new(window, cx)
                .auto_grow(2, 8)
                .placeholder("Comment on the pull request")
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
            tab: RightTab::Changes,
            open: HashMap::new(),
            review: HashMap::new(),
            review_to: HashMap::new(),
            draft: None,
            comment,
            pr_comment,
            lists: HashMap::new(),
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
            .w(px(WIDTH))
            .h_full()
            .flex_none()
            .flex()
            .flex_col()
            .bg(theme::panel())
            .border_l_1()
            .border_color(theme::border());
        let Screen::Conversation(id) = self.screen else {
            return frame
                .child(empty(
                    "Select a conversation to see its changes, branch diff and pull request.",
                ))
                .into_any_element();
        };
        let body = match self.right.tab {
            RightTab::Changes => self.render_diff(id, DiffSource::Changes, cx),
            RightTab::Branch => self.render_diff(id, DiffSource::Branch, cx),
            RightTab::Pr => self.render_pr(id, cx),
            RightTab::Info => self.render_info(id),
        };
        frame
            .child(self.right_tabs(id, cx))
            .child(div().flex_1().min_h_0().flex().flex_col().child(body))
            .children(self.render_review_bar(id, window, cx))
            .into_any_element()
    }

    fn right_tabs(&self, id: ConversationId, cx: &mut Context<Self>) -> AnyElement {
        let git = self.snapshot.git.get(&id);
        let count = |n: usize| (n > 0).then(|| n.to_string());
        let tabs = [
            (
                RightTab::Changes,
                "Changes",
                git.and_then(|g| count(g.changes.len())),
            ),
            (
                RightTab::Branch,
                "Branch",
                git.and_then(|g| count(g.branch_diff.len())),
            ),
            (
                RightTab::Pr,
                "PR",
                self.snapshot
                    .prs
                    .get(&id)
                    .map(|pr| format!("#{}", pr.number)),
            ),
            (RightTab::Info, "Info", None),
        ];
        div()
            .flex()
            .flex_none()
            .gap(px(2.))
            .px_2()
            .border_b_1()
            .border_color(theme::border())
            .children(tabs.into_iter().map(|(tab, text, n)| {
                let selected = self.right.tab == tab;
                div()
                    .id(SharedString::from(format!("right-tab-{text}")))
                    .px_2()
                    .pt(px(9.))
                    .pb(px(7.))
                    .border_b_2()
                    .border_color(if selected {
                        theme::accent()
                    } else {
                        gpui::transparent_black().into()
                    })
                    .text_size(px(12.5))
                    .text_color(if selected {
                        theme::fg()
                    } else {
                        theme::fg_muted()
                    })
                    .cursor_pointer()
                    .hover(|s| s.text_color(theme::fg()))
                    .flex()
                    .gap(px(5.))
                    .child(text)
                    .children(n.map(|n| {
                        div()
                            .text_size(px(11.))
                            .font_family(theme::MONO)
                            .text_color(theme::fg_faint())
                            .child(n)
                    }))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.right.tab = tab;
                        match tab {
                            RightTab::Changes | RightTab::Branch => {
                                this.core.send(Intent::RefreshGit(id));
                            }
                            RightTab::Pr | RightTab::Info => {}
                        }
                        cx.notify();
                    }))
            }))
            .into_any_element()
    }

    fn render_info(&self, id: ConversationId) -> AnyElement {
        let Some(view) = self.snapshot.conversation(id) else {
            return empty("This conversation no longer exists.");
        };
        let project = self
            .snapshot
            .projects
            .iter()
            .find(|p| p.id == view.conversation.project);
        let sessions: Vec<_> = view
            .sessions
            .iter()
            .filter_map(|s| self.snapshot.sessions.get(s))
            .collect();
        let git = self.snapshot.git.get(&id);
        let unset = || "CLI default".to_owned();
        let where_rows = [
            (
                "Project",
                project.map_or_default(|p| p.path.display().to_string()),
            ),
            (
                "Working dir",
                sessions
                    .first()
                    .map_or_default(|s| s.session.working_dir.display().to_string()),
            ),
            (
                "Branch",
                git.and_then(|g| g.branch.clone())
                    .unwrap_or_else(|| "none".into()),
            ),
            (
                "Base",
                git.and_then(|g| g.base.clone())
                    .unwrap_or_else(|| "none".into()),
            ),
        ];
        let session_rows = sessions.iter().map(|s| {
            let settings = &s.session.settings;
            key_values(vec![
                ("Agent", self.session_name(s)),
                ("CLI", settings.backend.display_name().to_owned()),
                ("Model", settings.model.clone().unwrap_or_else(unset)),
                ("Effort", settings.effort.clone().unwrap_or_else(unset)),
                ("Permission", settings.permission.clone()),
                (
                    "CLI session",
                    s.session
                        .cli_session
                        .clone()
                        .unwrap_or_else(|| "not started".into()),
                ),
            ])
            .pb_3()
        });
        div()
            .id("right-info")
            .flex_1()
            .overflow_y_scroll()
            .px_1()
            .child(label("Where"))
            .child(key_values(where_rows.into()))
            .child(label("Agent sessions"))
            .children(session_rows)
            .into_any_element()
    }

    fn render_review_bar(
        &self,
        id: ConversationId,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let count = self.right.review.get(&id).map_or(0, Vec::len);
        if count == 0 {
            return None;
        }
        let view = self.snapshot.conversation(id)?;
        let target = self.review_target(id)?;
        let choices = view
            .sessions
            .iter()
            .filter_map(|s| self.snapshot.sessions.get(s))
            .map(|s| (s.session.id.0.to_string(), self.session_name(s)))
            .collect();
        let current = self
            .snapshot
            .sessions
            .get(&target)
            .map_or_default(|s| self.session_name(s));
        let to = self.picker(
            "review-to",
            &current,
            choices,
            cx,
            move |this, value, _, _| {
                if let Ok(raw) = value.parse() {
                    this.right.review_to.insert(id, AgentSessionId(raw));
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
                .py(px(9.))
                .border_t_1()
                .border_color(theme::border())
                .bg(theme::surface())
                .text_size(px(12.))
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

    /// The session a review goes to: the one picked, else the one open in
    /// the main pane, else the first.
    fn review_target(&self, id: ConversationId) -> Option<AgentSessionId> {
        let sessions = &self.snapshot.conversation(id)?.sessions;
        self.right
            .review_to
            .get(&id)
            .copied()
            .filter(|s| sessions.contains(s))
            .or_else(|| self.current_session())
            .or_else(|| sessions.first().copied())
    }

    fn send_review(&mut self, id: ConversationId, cx: &mut Context<Self>) {
        let Some(session) = self.review_target(id) else {
            return;
        };
        let Some(pending) = self.right.review.remove(&id) else {
            return;
        };
        let comments: Vec<ReviewComment> = pending.into_iter().map(|p| p.comment).collect();
        self.core.send(Intent::Send {
            session,
            text: review_message(&comments),
        });
        cx.notify();
    }
}

/// Two columns: faint keys, monospace values.
fn key_values(rows: Vec<(&'static str, String)>) -> gpui::Div {
    div()
        .px_3()
        .flex()
        .flex_col()
        .gap(px(5.))
        .children(rows.into_iter().map(|(k, v)| {
            div()
                .flex()
                .gap_2()
                .text_size(px(12.))
                .child(
                    div()
                        .w(px(96.))
                        .flex_none()
                        .text_color(theme::fg_faint())
                        .child(k),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .font_family(theme::MONO)
                        .text_size(px(11.5))
                        .child(v),
                )
        }))
}
