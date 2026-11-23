//! My issues and my open pull requests (GH-2, GH-3): two rows at the top of
//! the left pane, each opening its list in the main pane. Issues are grouped
//! by board column (each column foldable), sorted by priority, and can be
//! filtered by sprint. A row opens the item's details in the drawer; an
//! issue starts a flow in the project whose repository it belongs
//! to, or opens the one already started for it.

use gpui::{
    AnyElement, Context, Div, Entity, FontWeight, IntoElement, Rgba, SharedString, Stateful,
    Window, div, prelude::*,
};
use gpui_component::input::TextareaState;
use leon_core::Intent;
use leon_domain::{
    BoardOption, FlowId, GhRef, Issue, PrState, ProjectId, PullRequest, issues_by_status,
};

use crate::gh_rows::{flow_link, pr_row};
use crate::icons::{Icon, icon};
use crate::pr_view::pr_state;
use crate::sprints::{SprintFilter, in_sprint, today};
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
            // Why the lists could not be read shows on their page, and
            // Doctor's red dot says what is missing (`gh`, its login): the
            // left pane stays quiet (DOCTOR-1).
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
            (Some(_), _) => None,
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
                div()
                    .id("gh-list")
                    .flex_1()
                    .overflow_y_scroll()
                    .child(crate::widgets::centered(
                        880.,
                        div()
                            .px_6()
                            .py_5()
                            .flex()
                            .flex_col()
                            .gap_5()
                            .children(
                                self.snapshot
                                    .github_error
                                    .clone()
                                    .map(crate::widgets::problem),
                            )
                            .children(empty_text.map(empty))
                            .children(sections),
                    )),
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
        let color = self.issue_color(status);
        let name = status.map_or("No status", |s| s.name.as_str()).to_owned();
        let title = div()
            .text_size(crate::theme::rem(13.5))
            .font_weight(FontWeight::SEMIBOLD)
            .child(name.clone());
        let rows = issues.iter().map(|i| self.issue_row(i, true, cx)).collect();
        self.group(name, color, title.into_any_element(), issues.len(), rows)
    }

    /// One repository of my pull requests: a foldable header, then its pull
    /// requests (GH-3).
    fn repo_group(&self, repo: &str, prs: &[&PullRequest], cx: &mut Context<Self>) -> AnyElement {
        let title = div()
            .font_family(theme::MONO)
            .text_size(crate::theme::rem(13.))
            .font_weight(FontWeight::SEMIBOLD)
            .child(leon_domain::repo_name(repo).to_owned());
        let color = theme::name_color(repo);
        let rows = prs
            .iter()
            .map(|pr| {
                let item = GhRef::pull_request(&pr.repo, pr.number);
                let link = self.flow_for_pr(pr).map(|f| flow_link(&item.text(), f, cx));
                pr_row(pr, crate::gh_rows::Fold::Never, link)
            })
            .collect();
        self.group(
            format!("repo:{repo}"),
            color,
            title.into_any_element(),
            prs.len(),
            rows,
        )
    }

    /// A foldable group of rows: chevron, a dot in `color`, `title`, how
    /// many (`count`); `key` keeps it folded across restarts.
    pub(crate) fn group(
        &self,
        key: String,
        color: Rgba,
        title: AnyElement,
        count: usize,
        rows: Vec<AnyElement>,
    ) -> AnyElement {
        self.group_with(key, color, title, count, rows, |header| header)
    }

    /// [`Self::group`], its header made by `header` from the usual one
    /// (e.g. to drag the group by it).
    pub(crate) fn group_with(
        &self,
        key: String,
        color: Rgba,
        title: AnyElement,
        count: usize,
        rows: Vec<AnyElement>,
        header: impl FnOnce(Stateful<Div>) -> Stateful<Div>,
    ) -> AnyElement {
        let folded = self.snapshot.folded_groups.contains(&key);
        div()
            .flex()
            .flex_col()
            .gap_1()
            .child(header(
                crate::widgets::fold_toggle(key, folded)
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .hover(|s| s.bg(theme::surface()))
                    .child(crate::widgets::mark(color, 10.))
                    .child(title)
                    .child(
                        div()
                            .px(crate::theme::rem(7.))
                            .rounded_full()
                            .bg(theme::surface_2())
                            .text_size(crate::theme::rem(11.5))
                            .text_color(theme::fg_muted())
                            .child(count.to_string()),
                    ),
            ))
            .when(!folded, |d| d.children(rows))
            .into_any_element()
    }

    /// What the drawer offers for an issue: "Open flow" when one was
    /// started for it, and always "Start flow" (GH-2).
    pub(crate) fn issue_actions(&self, issue: &Issue, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let item = GhRef::issue(&issue.repo, issue.number);
        let open = self.flow_for_issue(&item).map(|flow| {
            open_flow(
                SharedString::from(format!("gh-open-{}", item.text())),
                flow,
                cx,
            )
        });
        open.into_iter()
            .chain([self.start_flow_button(issue, cx)])
            .collect()
    }

    /// "Start flow" for an issue, in the project whose repository it
    /// belongs to; another one when a flow was already started for it.
    pub(crate) fn start_flow_button(&self, issue: &Issue, cx: &mut Context<Self>) -> AnyElement {
        let item = GhRef::issue(&issue.repo, issue.number);
        let project = self.project_for_repo(&issue.repo);
        button(
            SharedString::from(format!("gh-start-{}", item.text())),
            "Start flow",
            cx.listener(move |this, _, window, cx| {
                cx.stop_propagation();
                this.core.send(Intent::ShowDetail(None));
                this.open_new_flow(project, Some(item.clone()), window, cx);
            }),
        )
        .into_any_element()
    }

    /// The project whose `origin` is `repo` on GitHub.
    fn project_for_repo(&self, repo: &str) -> Option<ProjectId> {
        self.snapshot
            .project_repos
            .iter()
            .find(|(_, r)| r.eq_ignore_ascii_case(repo))
            .map(|(id, _)| *id)
    }

    pub(crate) fn flow_for_issue(&self, item: &GhRef) -> Option<FlowId> {
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
