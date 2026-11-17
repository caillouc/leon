//! My issues and my open pull requests (GH-2, GH-3): two rows at the top of
//! the left pane, each opening its list in the main pane. A row opens the
//! issue or PR on GitHub; an issue can start a conversation in the project
//! whose repository it belongs to.

use gpui::{
    AnyElement, Context, Div, FontWeight, IntoElement, Rgba, SharedString, div, prelude::*, px,
};
use leon_core::Intent;
use leon_domain::{
    CheckState, ConversationId, Issue, IssueStatus, PrState, ProjectId, PullRequest,
    issues_by_status,
};

use crate::left::ago;
use crate::pr_view::{check_color, labels, pr_state};
use crate::theme;
use crate::widgets::{button, empty, label};
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
            Self::Issues => "Open, assigned to you, by board status",
            Self::PullRequests => "Open, written by you",
        }
    }
}

impl Workspace {
    /// The GitHub section of the left pane.
    pub(crate) fn render_github_rows(&self, cx: &mut Context<Self>) -> AnyElement {
        let row = |list: GhList, count: usize, cx: &mut Context<Self>| {
            let selected = self.screen == Screen::GitHub(list);
            div()
                .id(SharedString::from(list.title()))
                .mx_1()
                .px_2()
                .py(px(5.))
                .rounded_md()
                .flex()
                .cursor_pointer()
                .when(selected, |s| s.bg(theme::accent_soft()))
                .when(!selected, |s| s.hover(|s| s.bg(theme::surface_2())))
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
            .child(row(GhList::Issues, self.snapshot.my_issues.len(), cx))
            .child(row(GhList::PullRequests, self.snapshot.my_prs.len(), cx))
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
        let count = match list {
            GhList::Issues => self.snapshot.my_issues.len(),
            GhList::PullRequests => self.snapshot.my_prs.len(),
        };
        let sections: Vec<AnyElement> = match list {
            GhList::Issues => issues_by_status(&self.snapshot.my_issues)
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
            (None, GhList::Issues) => Some("No open issues assigned to you.".to_owned()),
            (None, GhList::PullRequests) => Some("No open pull requests by you.".to_owned()),
        };
        div()
            .size_full()
            .flex()
            .flex_col()
            .child(header(list, count, cx))
            .child(
                div().id("gh-list").flex_1().overflow_y_scroll().child(
                    div()
                        .max_w(px(860.))
                        .w_full()
                        .mx_auto()
                        .px_6()
                        .py_5()
                        .flex()
                        .flex_col()
                        .gap_6()
                        .children(empty_text.map(empty))
                        .children(sections),
                ),
            )
            .into_any_element()
    }

    /// One board column: its name and color, then its issues.
    fn issue_group(
        &self,
        status: Option<&IssueStatus>,
        issues: &[&Issue],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let color = theme::github_color(status.and_then(|s| s.color.as_deref()));
        let name = status.map_or("No status", |s| s.name.as_str());
        div()
            .flex()
            .flex_col()
            .gap_1()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_3()
                    .pb_1()
                    .child(div().size(px(9.)).rounded_full().bg(color))
                    .child(
                        div()
                            .text_size(px(13.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(name.to_owned()),
                    )
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(theme::fg_faint())
                            .child(issues.len().to_string()),
                    ),
            )
            .children(issues.iter().map(|i| self.issue_row(i, color, cx)))
            .into_any_element()
    }

    fn issue_row(&self, issue: &Issue, color: Rgba, cx: &mut Context<Self>) -> AnyElement {
        let project = self.project_for_repo(&issue.repo);
        let message = format!(
            "Issue {}#{}: {}\n{}\n\n",
            issue.repo, issue.number, issue.title, issue.url
        );
        let start = button(
            SharedString::from(format!("gh-start-{}-{}", issue.repo, issue.number)),
            "Start conversation",
            cx.listener(move |this, _, window, cx| {
                cx.stop_propagation();
                this.open_new_conversation(project, Some(&message), window, cx);
            }),
        );
        let icon = div()
            .size(px(14.))
            .rounded_full()
            .border_2()
            .border_color(color)
            .flex()
            .items_center()
            .justify_center()
            .child(div().size(px(4.)).rounded_full().bg(color));
        let meta = div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap_2()
            .child(repo_text(&issue.repo))
            .child(labels(&issue.labels));
        Row {
            id: SharedString::from(format!("gh-issue-{}-{}", issue.repo, issue.number)),
            url: issue.url.clone(),
            icon: icon.into_any_element(),
            title: issue.title.clone(),
            number: issue.number,
            age: format!("{} ago", ago(issue.updated_at)),
            meta,
            action: start.into_any_element(),
        }
        .render(cx)
    }

    fn pr_row(&self, pr: &PullRequest, cx: &mut Context<Self>) -> AnyElement {
        let (color, state) = pr_state(pr.state);
        let conversation = self.conversation_for_pr(pr);
        let action = match conversation {
            Some(id) => button(
                SharedString::from(format!("gh-open-{}-{}", pr.repo, pr.number)),
                "Open conversation",
                cx.listener(move |this, _, _, cx| {
                    cx.stop_propagation();
                    this.select(id, cx);
                }),
            )
            .into_any_element(),
            None => div().into_any_element(),
        };
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
        Row {
            id: SharedString::from(format!("gh-pr-{}-{}", pr.repo, pr.number)),
            url: pr.url.clone(),
            icon: pill(color, state, pr.state == PrState::Draft).into_any_element(),
            title: pr.title.clone(),
            number: pr.number,
            age: String::new(),
            meta,
            action,
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

    fn conversation_for_pr(&self, pr: &PullRequest) -> Option<ConversationId> {
        self.snapshot
            .prs
            .iter()
            .find(|(_, p)| p.repo == pr.repo && p.number == pr.number)
            .map(|(id, _)| *id)
    }
}

/// The list's title, what it holds, its size and Refresh.
fn header(list: GhList, count: usize, cx: &mut Context<Workspace>) -> Div {
    div()
        .flex_none()
        .px_6()
        .py_3()
        .flex()
        .items_center()
        .gap_3()
        .border_b_1()
        .border_color(theme::border())
        .child(
            div()
                .flex()
                .flex_col()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .text_size(px(15.))
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(list.title()),
                        )
                        .child(
                            div()
                                .px(px(7.))
                                .rounded_full()
                                .bg(theme::surface_2())
                                .text_size(px(11.5))
                                .text_color(theme::fg_muted())
                                .child(count.to_string()),
                        ),
                )
                .child(
                    div()
                        .text_size(px(12.))
                        .text_color(theme::fg_faint())
                        .child(list.subtitle()),
                ),
        )
        .child(div().flex_1())
        .child(button(
            "gh-refresh",
            "Refresh",
            cx.listener(|this, _, _, _| this.core.send(Intent::RefreshGitHub)),
        ))
}

/// A list row: icon, title and number, age; under it the details; the
/// action on the right. Clicking anywhere else opens `url` on GitHub.
struct Row {
    id: SharedString,
    url: String,
    icon: AnyElement,
    title: String,
    number: u64,
    age: String,
    meta: Div,
    action: AnyElement,
}

impl Row {
    fn render(self, cx: &mut Context<Workspace>) -> AnyElement {
        let Self {
            id,
            url,
            icon,
            title,
            number,
            age,
            meta,
            action,
        } = self;
        div()
            .id(id)
            .px_3()
            .py(px(10.))
            .rounded_lg()
            .flex()
            .items_center()
            .gap_3()
            .cursor_pointer()
            .hover(|s| s.bg(theme::surface()))
            .on_click(cx.listener(move |_, _, _, cx| cx.open_url(&url)))
            .child(
                div()
                    .w(px(56.))
                    .flex_none()
                    .flex()
                    .justify_center()
                    .child(icon),
            )
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
            .child(div().flex_none().child(action))
            .into_any_element()
    }
}

fn repo_text(repo: &str) -> Div {
    div()
        .font_family(theme::MONO)
        .text_size(px(11.5))
        .child(repo.to_owned())
}

/// A pull request's state as a tinted pill; a draft is outlined only.
fn pill(color: Rgba, text: &'static str, outlined: bool) -> Div {
    div()
        .px(px(9.))
        .py(px(2.))
        .rounded_full()
        .border_1()
        .border_color(Rgba { a: 0.6, ..color })
        .when(!outlined, |d| d.bg(Rgba { a: 0.18, ..color }))
        .text_size(px(11.5))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(color)
        .child(text)
}

/// The checks' sum: a dot in their color and what it means.
fn checks(state: CheckState) -> Div {
    let text = match state {
        CheckState::Pass => "Checks passing",
        CheckState::Fail => "Checks failing",
        CheckState::Pending => "Checks running",
    };
    let color = check_color(state);
    div()
        .flex()
        .items_center()
        .gap(px(5.))
        .text_color(color)
        .child(div().size(px(7.)).rounded_full().bg(color))
        .child(text)
}
