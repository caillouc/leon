//! My issues and my open pull requests (GH-2, GH-3): two rows at the top of
//! the left pane, each opening its list in the main pane.

use gpui::{AnyElement, Context, FontWeight, IntoElement, SharedString, div, prelude::*, px};
use leon_core::Intent;
use leon_domain::ConversationId;

use crate::left::ago;
use crate::pr_view::{labels, pr_state};
use crate::theme;
use crate::widgets::{empty, label};
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
}

/// One item of a list, whatever its kind.
struct Item {
    repo: String,
    number: u64,
    title: String,
    labels: Vec<String>,
    status: Option<String>,
    /// The conversation working on it, if any.
    conversation: Option<ConversationId>,
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
        let items: Vec<Item> = match list {
            GhList::Issues => self
                .snapshot
                .my_issues
                .iter()
                .map(|i| Item {
                    repo: i.repo.clone(),
                    number: i.number,
                    title: i.title.clone(),
                    labels: i.labels.clone(),
                    status: Some(format!("updated {} ago", ago(i.updated_at))),
                    conversation: None,
                })
                .collect(),
            GhList::PullRequests => self
                .snapshot
                .my_prs
                .iter()
                .map(|pr| Item {
                    repo: pr.repo.clone(),
                    number: pr.number,
                    title: pr.title.clone(),
                    labels: pr.labels.clone(),
                    status: Some(pr_state(pr.state).1.to_owned()),
                    conversation: self
                        .snapshot
                        .prs
                        .iter()
                        .find(|(_, p)| p.repo == pr.repo && p.number == pr.number)
                        .map(|(id, _)| *id),
                })
                .collect(),
        };
        let header = div()
            .px_6()
            .py_3()
            .border_b_1()
            .border_color(theme::border())
            .child(
                div()
                    .text_size(px(15.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(list.title()),
            );
        let rows = items.into_iter().enumerate().map(|(ix, item)| {
            let open = item.conversation;
            div()
                .id(("gh-item", ix))
                .px_6()
                .py_2()
                .border_b_1()
                .border_color(theme::border())
                .flex()
                .flex_col()
                .gap_1()
                .when(open.is_some(), |d| {
                    d.cursor_pointer().hover(|s| s.bg(theme::surface()))
                })
                .child(
                    div()
                        .flex()
                        .gap_2()
                        .child(div().font_weight(FontWeight::MEDIUM).child(item.title))
                        .child(
                            div()
                                .text_color(theme::fg_faint())
                                .child(format!("#{}", item.number)),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .gap_2()
                        .text_size(px(12.))
                        .text_color(theme::fg_muted())
                        .child(div().font_family(theme::MONO).child(item.repo))
                        .children(item.status)
                        .child(labels(&item.labels))
                        .children(open.map(|_| "Has a conversation")),
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    if let Some(id) = open {
                        this.select(id, cx);
                    }
                }))
        });
        let body = div()
            .id("gh-list")
            .flex_1()
            .overflow_y_scroll()
            .child(div().max_w(px(760.)).mx_auto().children(rows));
        let empty_text = match (&self.snapshot.github_error, list) {
            (Some(error), _) => Some(error.clone()),
            (None, _) if self.list_len(list) > 0 => None,
            (None, GhList::Issues) => Some("No open issues assigned to you.".to_owned()),
            (None, GhList::PullRequests) => Some("No open pull requests by you.".to_owned()),
        };
        div()
            .size_full()
            .flex()
            .flex_col()
            .child(header)
            .children(empty_text.map(empty))
            .child(body)
            .into_any_element()
    }

    fn list_len(&self, list: GhList) -> usize {
        match list {
            GhList::Issues => self.snapshot.my_issues.len(),
            GhList::PullRequests => self.snapshot.my_prs.len(),
        }
    }
}
