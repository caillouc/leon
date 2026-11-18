//! My issues and my open pull requests (GH-2, GH-3): two rows at the top of
//! the left pane, each opening its list in the main pane. Issues are grouped
//! by board column (each column foldable), sorted by priority, and can be
//! filtered by sprint. A row opens the item's details in the drawer; an
//! issue starts a conversation in the project whose repository it belongs
//! to, or opens the one already started for it.

use std::collections::HashSet;

use gpui::{
    AnyElement, Context, Div, Entity, FontWeight, IntoElement, Rgba, SharedString, Window, div,
    prelude::*, px,
};
use gpui_component::input::TextareaState;
use leon_core::Intent;
use leon_domain::{
    BoardOption, CheckState, ConversationId, GhRef, Issue, PrState, ProjectId, PullRequest,
    issues_by_status,
};

use crate::icons::{Icon, icon};
use crate::left::since;
use crate::pr_view::{check_color, labels, pr_state};
use crate::sprints::{SprintFilter, board_chips, in_sprint, today};
use crate::theme;
use crate::widgets::{button, empty, label, pane_header};
use crate::workspace::{Screen, Workspace};

/// Which GitHub list the main pane shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GhList {
    Issues,
    PullRequests,
}

impl GhList {
    fn title(self) -> &'static str {
        match self {
            Self::Issues => "My issues",
            Self::PullRequests => "My open pull requests",
        }
    }

    fn subtitle(self) -> &'static str {
        match self {
            Self::Issues => "Open, assigned to you, by board column and priority",
            Self::PullRequests => "Open, written by you",
        }
    }
}

/// What the GitHub lists remember while the window is open.
pub(crate) struct GhState {
    /// Board columns folded by the user, by name.
    pub folded: HashSet<String>,
    /// The sprint shown.
    pub sprint: SprintFilter,
    /// The comment box of the details drawer.
    pub comment: Entity<TextareaState>,
}

impl GhState {
    pub(crate) fn new(window: &mut Window, cx: &mut Context<Workspace>) -> Self {
        Self {
            folded: HashSet::new(),
            sprint: SprintFilter::Current,
            comment: cx.new(|cx| {
                TextareaState::new(window, cx)
                    .auto_grow(2, 8)
                    .placeholder("Comment on GitHub")
            }),
        }
    }
}

impl Workspace {
    /// The GitHub section of the left pane.
    pub(crate) fn render_github_rows(&self, cx: &mut Context<Self>) -> AnyElement {
        let row = |list: GhList, glyph: Icon, count: usize, cx: &mut Context<Self>| {
            let selected = self.screen == Screen::GitHub(list);
            div()
                .id(SharedString::from(list.title()))
                .mx_1()
                .px_2()
                .py(px(5.))
                .rounded_md()
                .flex()
                .items_center()
                .gap_2()
                .cursor_pointer()
                .when(selected, |s| s.bg(theme::accent_soft()))
                .when(!selected, |s| s.hover(|s| s.bg(theme::surface_2())))
                .child(icon(glyph, theme::fg_muted(), 14.))
                .child(div().flex_1().child(list.title()))
                .child(div().text_color(theme::fg_faint()).child(count.to_string()))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.screen = Screen::GitHub(list);
                    this.core.send(Intent::View(None));
                    this.core.send(Intent::RefreshGitHub);
                    cx.notify();
                }))
        };
        div()
            .child(label("GitHub"))
            .child(row(
                GhList::Issues,
                Icon::Issue,
                self.snapshot.my_issues.len(),
                cx,
            ))
            .child(row(
                GhList::PullRequests,
                Icon::PullRequest,
                self.snapshot.my_prs.len(),
                cx,
            ))
            .children(
                self.snapshot
                    .github_error
                    .as_ref()
                    .map(|e| empty(e.clone())),
            )
            .into_any_element()
    }

    /// A GitHub list in the main pane.
    pub(crate) fn render_github_list(&self, list: GhList, cx: &mut Context<Self>) -> AnyElement {
        let today = today();
        let shown = self.shown_sprint(today);
        let issues: Vec<Issue> = self
            .snapshot
            .my_issues
            .iter()
            .filter(|i| in_sprint(i, shown.as_ref()))
            .cloned()
            .collect();
        let count = match list {
            GhList::Issues => issues.len(),
            GhList::PullRequests => self.snapshot.my_prs.len(),
        };
        let sections: Vec<AnyElement> = match list {
            GhList::Issues => issues_by_status(&issues)
                .into_iter()
                .map(|(status, issues)| self.issue_group(status, &issues, cx))
                .collect(),
            GhList::PullRequests => vec![
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .children(self.snapshot.my_prs.iter().map(|pr| self.pr_row(pr, cx)))
                    .into_any_element(),
            ],
        };
        let empty_text = match (&self.snapshot.github_error, list) {
            (Some(error), _) => Some(error.clone()),
            (None, _) if count > 0 => None,
            (None, GhList::Issues) if shown.is_some() => {
                Some("No open issue of yours in this sprint.".to_owned())
            }
            (None, GhList::Issues) => Some("No open issues assigned to you.".to_owned()),
            (None, GhList::PullRequests) => Some("No open pull requests by you.".to_owned()),
        };
        let filter = (list == GhList::Issues)
            .then(|| self.sprint_filter(shown.as_ref(), today, cx))
            .flatten();
        div()
            .size_full()
            .flex()
            .flex_col()
            .child(header(list, count, filter, cx))
            .child(
                div().id("gh-list").flex_1().overflow_y_scroll().child(
                    div()
                        .max_w(px(880.))
                        .w_full()
                        .mx_auto()
                        .px_6()
                        .py_5()
                        .flex()
                        .flex_col()
                        .gap_5()
                        .children(empty_text.map(empty))
                        .children(sections),
                ),
            )
            .into_any_element()
    }

    /// One board column: a foldable header, then its issues.
    fn issue_group(
        &self,
        status: Option<&BoardOption>,
        issues: &[&Issue],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let color = status.map_or(theme::fg_faint(), |s| self.status_color(s));
        let name = status.map_or("No status", |s| s.name.as_str()).to_owned();
        let folded = self.gh.folded.contains(&name);
        let key = name.clone();
        div()
            .flex()
            .flex_col()
            .gap_1()
            .child(
                div()
                    .id(SharedString::from(format!("gh-group-{name}")))
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .cursor_pointer()
                    .hover(|s| s.bg(theme::surface()))
                    .child(icon(
                        if folded {
                            Icon::ChevronRight
                        } else {
                            Icon::ChevronDown
                        },
                        theme::fg_muted(),
                        15.,
                    ))
                    .child(div().size(px(10.)).rounded_full().bg(color))
                    .child(
                        div()
                            .text_size(px(13.5))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(name),
                    )
                    .child(
                        div()
                            .px(px(7.))
                            .rounded_full()
                            .bg(theme::surface_2())
                            .text_size(px(11.5))
                            .text_color(theme::fg_muted())
                            .child(issues.len().to_string()),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if !this.gh.folded.remove(&key) {
                            this.gh.folded.insert(key.clone());
                        }
                        cx.notify();
                    })),
            )
            .when(!folded, |d| {
                d.children(issues.iter().map(|i| self.issue_row(i, color, cx)))
            })
            .into_any_element()
    }

    fn issue_row(&self, issue: &Issue, color: Rgba, cx: &mut Context<Self>) -> AnyElement {
        let meta = div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap_2()
            .child(repo_text(&issue.repo))
            .children(board_chips(issue))
            .child(labels(&issue.labels));
        let item = GhRef::issue(&issue.repo, issue.number);
        // A linked conversation is a link in the details line; otherwise
        // "Start conversation" floats over the row's end on hover.
        let linked = self.conversation_for_issue(&item);
        let meta = meta.children(linked.map(|c| conversation_link(&item, c, cx)));
        Row {
            hover: linked.is_none().then(|| self.issue_action(issue, cx)),
            item,
            icon: icon(Icon::Issue, color, 16.).into_any_element(),
            title: issue.title.clone(),
            age: since(issue.updated_at),
            meta,
        }
        .render(cx)
    }

    /// "Open conversation" when one was started for the issue, else "Start
    /// conversation" in the project whose repository it belongs to.
    pub(crate) fn issue_action(&self, issue: &Issue, cx: &mut Context<Self>) -> AnyElement {
        let item = GhRef::issue(&issue.repo, issue.number);
        let id = SharedString::from(format!("gh-act-{}", item.text()));
        if let Some(conversation) = self.conversation_for_issue(&item) {
            return open_conversation(id, conversation, cx);
        }
        let project = self.project_for_repo(&issue.repo);
        button(
            id,
            "Start conversation",
            cx.listener(move |this, _, window, cx| {
                cx.stop_propagation();
                this.core.send(Intent::ShowDetail(None));
                this.open_new_conversation(project, Some(item.clone()), window, cx);
            }),
        )
        .into_any_element()
    }

    fn pr_row(&self, pr: &PullRequest, cx: &mut Context<Self>) -> AnyElement {
        let meta = div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap_2()
            .child(repo_text(&pr.repo))
            .child(
                div()
                    .font_family(theme::MONO)
                    .text_size(px(11.5))
                    .text_color(theme::fg_faint())
                    .child(format!("{} → {}", pr.branch, pr.base)),
            )
            .children(pr.checks_state().map(checks))
            .child(labels(&pr.labels));
        let item = GhRef::pull_request(&pr.repo, pr.number);
        let meta = meta.children(
            self.conversation_for_pr(pr)
                .map(|c| conversation_link(&item, c, cx)),
        );
        Row {
            hover: None,
            item,
            icon: pr_icon(pr.state, 16.).into_any_element(),
            title: pr.title.clone(),
            age: pr_state(pr.state).1.to_owned(),
            meta,
        }
        .render(cx)
    }

    /// "Open conversation" when a conversation works on the pull request.
    pub(crate) fn pr_action(&self, pr: &PullRequest, cx: &mut Context<Self>) -> AnyElement {
        match self.conversation_for_pr(pr) {
            Some(conversation) => open_conversation(
                SharedString::from(format!("gh-open-{}-{}", pr.repo, pr.number)),
                conversation,
                cx,
            ),
            None => div().into_any_element(),
        }
    }

    /// The project whose `origin` is `repo` on GitHub.
    fn project_for_repo(&self, repo: &str) -> Option<ProjectId> {
        self.snapshot
            .project_repos
            .iter()
            .find(|(_, r)| r.eq_ignore_ascii_case(repo))
            .map(|(id, _)| *id)
    }

    fn conversation_for_issue(&self, item: &GhRef) -> Option<ConversationId> {
        self.snapshot
            .conversations
            .iter()
            .find(|c| c.conversation.issue.as_ref() == Some(item))
            .map(|c| c.conversation.id)
    }

    fn conversation_for_pr(&self, pr: &PullRequest) -> Option<ConversationId> {
        self.snapshot
            .prs
            .iter()
            .find(|(_, p)| p.repo == pr.repo && p.number == pr.number)
            .map(|(id, _)| *id)
    }
}

fn open_conversation(
    id: SharedString,
    conversation: ConversationId,
    cx: &mut Context<Workspace>,
) -> AnyElement {
    button(
        id,
        "Open conversation",
        cx.listener(move |this, _, _, cx| {
            cx.stop_propagation();
            this.core.send(Intent::ShowDetail(None));
            this.select(conversation, cx);
        }),
    )
    .into_any_element()
}

/// A pull request's icon in its state's colour.
pub(crate) fn pr_icon(state: PrState, size: f32) -> gpui::Svg {
    let glyph = match state {
        PrState::Open => Icon::PullRequest,
        PrState::Draft => Icon::PullRequestDraft,
        PrState::Merged => Icon::Merged,
        PrState::Closed => Icon::PullRequestClosed,
    };
    icon(glyph, pr_state(state).0, size)
}

/// The checks summed up: an icon in their colour and what it means.
pub(crate) fn checks(state: CheckState) -> Div {
    let color = check_color(state);
    let (glyph, text) = match state {
        CheckState::Pass => (Icon::Passed, "Checks passing"),
        CheckState::Fail => (Icon::Failed, "Checks failing"),
        CheckState::Pending => (Icon::Pending, "Checks running"),
    };
    div()
        .flex()
        .items_center()
        .gap(px(4.))
        .text_color(color)
        .child(icon(glyph, color, 12.))
        .child(text)
}

/// The list's title, its size and what it holds, the filter and Refresh.
fn header(
    list: GhList,
    count: usize,
    filter: Option<AnyElement>,
    cx: &mut Context<Workspace>,
) -> Div {
    pane_header(list.title(), Some(format!("{count} · {}", list.subtitle())))
        .children(filter)
        .child(button(
            "gh-refresh",
            "Refresh",
            cx.listener(|this, _, _, _| this.core.send(Intent::RefreshGitHub)),
        ))
}

/// A list row: icon, title, age; under it the details. Clicking it opens
/// the item's details. `hover` floats over the row's right end while the
/// mouse is on the row, so it takes no room otherwise.
struct Row {
    item: GhRef,
    hover: Option<AnyElement>,
    icon: AnyElement,
    title: String,
    age: String,
    meta: Div,
}

impl Row {
    fn render(self, cx: &mut Context<Workspace>) -> AnyElement {
        let Self {
            item,
            hover,
            icon,
            title,
            age,
            meta,
        } = self;
        let number = item.number;
        let group = SharedString::from(format!("gh-row-{}", item.text()));
        div()
            .id(group.clone())
            .group(group.clone())
            .relative()
            .px_3()
            .py(px(10.))
            .rounded_lg()
            .flex()
            .items_start()
            .gap_3()
            .cursor_pointer()
            .hover(|s| s.bg(theme::surface()))
            .on_click(cx.listener(move |this, _, _, _| {
                this.core.send(Intent::ShowDetail(Some(item.clone())));
            }))
            .child(div().pt(px(2.)).child(icon))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(5.))
                    .child(
                        div()
                            .flex()
                            .items_baseline()
                            .gap_2()
                            .child(div().min_w_0().font_weight(FontWeight::MEDIUM).child(title))
                            .child(
                                div()
                                    .flex_none()
                                    .text_size(px(12.))
                                    .text_color(theme::fg_faint())
                                    .child(format!("#{number}")),
                            )
                            .child(div().flex_1())
                            .child(
                                div()
                                    .flex_none()
                                    .text_size(px(11.5))
                                    .text_color(theme::fg_faint())
                                    .child(age),
                            ),
                    )
                    .child(meta.text_size(px(12.)).text_color(theme::fg_muted())),
            )
            .children(hover.map(|action| {
                div()
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .right_3()
                    .flex()
                    .items_center()
                    .opacity(0.)
                    .group_hover(group, |s| s.opacity(1.))
                    .child(div().pl_4().bg(theme::surface()).child(action))
            }))
            .into_any_element()
    }
}

fn repo_text(repo: &str) -> Div {
    div()
        .font_family(theme::MONO)
        .text_size(px(11.5))
        .child(repo.to_owned())
}

/// "Open conversation" as a link in a row's details line.
fn conversation_link(
    item: &GhRef,
    conversation: ConversationId,
    cx: &mut Context<Workspace>,
) -> AnyElement {
    div()
        .id(SharedString::from(format!("gh-link-{}", item.text())))
        .flex()
        .items_center()
        .gap(px(4.))
        .text_color(theme::accent())
        .cursor_pointer()
        .hover(|s| s.opacity(0.8))
        .child(icon(Icon::Conversation, theme::accent(), 12.))
        .child("Open conversation")
        .on_click(cx.listener(move |this, _, _, cx| {
            cx.stop_propagation();
            this.select(conversation, cx);
        }))
        .into_any_element()
}
