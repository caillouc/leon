//! My issues and my open pull requests (GH-2, GH-3): two rows at the top of
//! the left pane, each opening its list in the main pane. Issues are grouped
//! by board column (each column foldable), sorted by priority, and can be
//! filtered by sprint. A row opens the item's details in the drawer; an
//! issue starts a flow in the project whose repository it belongs
//! to, or opens the one already started for it.

use gpui::{
    AnyElement, Context, Div, Entity, FontWeight, IntoElement, Rgba, SharedString, Window, div,
    prelude::*,
};
use gpui_component::input::TextareaState;
use leon_core::Intent;
use leon_domain::{
    BoardOption, CheckState, FlowId, GhRef, Issue, PrState, ProjectId, PullRequest,
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
    /// The sprint shown.
    pub sprint: SprintFilter,
    /// The comment box of the details drawer.
    pub comment: Entity<TextareaState>,
}

impl GhState {
    pub(crate) fn new(window: &mut Window, cx: &mut Context<Workspace>) -> Self {
        Self {
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
                .py(crate::theme::rem(5.))
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
            GhList::PullRequests => leon_domain::prs_by_repo(&self.snapshot.my_prs)
                .into_iter()
                .map(|(repo, prs)| self.repo_group(repo, &prs, cx))
                .collect(),
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
                        .max_w(crate::theme::rem(880.))
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
        let title = div()
            .text_size(crate::theme::rem(13.5))
            .font_weight(FontWeight::SEMIBOLD)
            .child(name.clone());
        let rows = issues
            .iter()
            .map(|i| self.issue_row(i, color, cx))
            .collect();
        self.group(name, color, title.into_any_element(), rows, cx)
    }

    /// One repository of my pull requests: a foldable header, then its pull
    /// requests (GH-3).
    fn repo_group(&self, repo: &str, prs: &[&PullRequest], cx: &mut Context<Self>) -> AnyElement {
        let (owner, name) = repo.split_once('/').unwrap_or(("", repo));
        let title = div()
            .flex()
            .items_center()
            .font_family(theme::MONO)
            .text_size(crate::theme::rem(13.))
            .child(
                div()
                    .text_color(theme::fg_muted())
                    .child(format!("{owner}/")),
            )
            .child(
                div()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(name.to_owned()),
            );
        let color = theme::agent_color(repo.bytes().map(usize::from).sum());
        let rows = prs.iter().map(|pr| self.pr_row(pr, cx)).collect();
        self.group(
            format!("repo:{repo}"),
            color,
            title.into_any_element(),
            rows,
            cx,
        )
    }

    /// A foldable group of rows: chevron, a dot in `color`, `title`, the
    /// count; `key` remembers it folded while the window is open.
    fn group(
        &self,
        key: String,
        color: Rgba,
        title: AnyElement,
        rows: Vec<AnyElement>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        // Kept by the core across restarts.
        let folded = self.snapshot.folded_groups.contains(&key);
        let count = rows.len();
        div()
            .flex()
            .flex_col()
            .gap_1()
            .child(
                div()
                    .id(SharedString::from(format!("gh-group-{key}")))
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
                    .child(div().size(crate::theme::rem(10.)).rounded_full().bg(color))
                    .child(title)
                    .child(
                        div()
                            .px(crate::theme::rem(7.))
                            .rounded_full()
                            .bg(theme::surface_2())
                            .text_size(crate::theme::rem(11.5))
                            .text_color(theme::fg_muted())
                            .child(count.to_string()),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.core.send(Intent::FoldGroup(key.clone(), !folded));
                        cx.notify();
                    })),
            )
            .when(!folded, |d| d.children(rows))
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
        // A linked flow is a link in the details line; otherwise
        // "Start flow" floats over the row's end on hover.
        let linked = self.flow_for_issue(&item);
        let meta = meta.children(linked.map(|c| flow_link(&item, c, cx)));
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

    /// "Open flow" when one was started for the issue, else "Start
    /// flow" in the project whose repository it belongs to.
    pub(crate) fn issue_action(&self, issue: &Issue, cx: &mut Context<Self>) -> AnyElement {
        let item = GhRef::issue(&issue.repo, issue.number);
        let id = SharedString::from(format!("gh-act-{}", item.text()));
        if let Some(flow) = self.flow_for_issue(&item) {
            return open_flow(id, flow, cx);
        }
        let project = self.project_for_repo(&issue.repo);
        button(
            id,
            "Start conversation",
            cx.listener(move |this, _, window, cx| {
                cx.stop_propagation();
                this.core.send(Intent::ShowDetail(None));
                this.open_new_flow(project, Some(item.clone()), window, cx);
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
            .child(
                div()
                    .font_family(theme::MONO)
                    .text_size(crate::theme::rem(11.5))
                    .text_color(theme::fg_faint())
                    .child(format!("{} → {}", pr.branch, pr.base)),
            )
            .children(pr.checks_state().map(checks))
            .child(labels(&pr.labels));
        let item = GhRef::pull_request(&pr.repo, pr.number);
        let meta = meta.children(self.flow_for_pr(pr).map(|c| flow_link(&item, c, cx)));
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

    /// The project whose `origin` is `repo` on GitHub.
    fn project_for_repo(&self, repo: &str) -> Option<ProjectId> {
        self.snapshot
            .project_repos
            .iter()
            .find(|(_, r)| r.eq_ignore_ascii_case(repo))
            .map(|(id, _)| *id)
    }

    fn flow_for_issue(&self, item: &GhRef) -> Option<FlowId> {
        self.snapshot
            .flows
            .iter()
            .find(|c| c.issues.contains(item))
            .map(|c| c.flow.id)
    }

    fn flow_for_pr(&self, pr: &PullRequest) -> Option<FlowId> {
        self.snapshot
            .flows
            .iter()
            .find(|c| {
                self.snapshot
                    .prs_of(c)
                    .iter()
                    .any(|p| p.repo == pr.repo && p.number == pr.number)
            })
            .map(|c| c.flow.id)
    }
}

fn open_flow(id: SharedString, flow: FlowId, cx: &mut Context<Workspace>) -> AnyElement {
    button(
        id,
        "Open flow",
        cx.listener(move |this, _, _, cx| {
            cx.stop_propagation();
            this.core.send(Intent::ShowDetail(None));
            this.select(flow, cx);
        }),
    )
    .into_any_element()
}

/// A pull request state's icon.
pub(crate) fn pr_glyph(state: PrState) -> Icon {
    match state {
        PrState::Open => Icon::PullRequest,
        PrState::Draft => Icon::PullRequestDraft,
        PrState::Merged => Icon::Merged,
        PrState::Closed => Icon::PullRequestClosed,
    }
}

/// A pull request's icon in its state's colour.
pub(crate) fn pr_icon(state: PrState, size: f32) -> gpui::Svg {
    icon(pr_glyph(state), pr_state(state).0, size)
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
        .gap(crate::theme::rem(4.))
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
            .py(crate::theme::rem(10.))
            .rounded_lg()
            .flex()
            .items_start()
            .gap_3()
            .cursor_pointer()
            .hover(|s| s.bg(theme::surface()))
            .on_click(cx.listener(move |this, _, _, _| {
                this.core.send(Intent::ShowDetail(Some(item.clone())));
            }))
            .child(div().pt(crate::theme::rem(2.)).child(icon))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(crate::theme::rem(5.))
                    .child(
                        div()
                            .flex()
                            .items_baseline()
                            .gap_2()
                            .child(div().min_w_0().font_weight(FontWeight::MEDIUM).child(title))
                            .child(
                                div()
                                    .flex_none()
                                    .text_size(crate::theme::rem(12.))
                                    .text_color(theme::fg_faint())
                                    .child(format!("#{number}")),
                            )
                            .child(div().flex_1())
                            .child(
                                div()
                                    .flex_none()
                                    .text_size(crate::theme::rem(11.5))
                                    .text_color(theme::fg_faint())
                                    .child(age),
                            ),
                    )
                    .child(
                        meta.text_size(crate::theme::rem(12.))
                            .text_color(theme::fg_muted()),
                    ),
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
        .text_size(crate::theme::rem(11.5))
        .child(repo.to_owned())
}

/// "Open flow" as a link in a row's details line.
fn flow_link(item: &GhRef, flow: FlowId, cx: &mut Context<Workspace>) -> AnyElement {
    div()
        .id(SharedString::from(format!("gh-link-{}", item.text())))
        .flex()
        .items_center()
        .gap(crate::theme::rem(4.))
        .text_color(theme::accent())
        .cursor_pointer()
        .hover(|s| s.opacity(0.8))
        .child(icon(Icon::Chat, theme::accent(), 12.))
        .child("Open flow")
        .on_click(cx.listener(move |this, _, _, cx| {
            cx.stop_propagation();
            this.select(flow, cx);
        }))
        .into_any_element()
}
