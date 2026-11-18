//! An issue's or a pull request's details in the drawer (GH-2, GH-3): read
//! from GitHub when a row of My issues or My open pull requests is clicked,
//! with what can be done from Leon — start or open the flow, request
//! a Copilot review, comment — and a link to GitHub.

use gpui::{AnyElement, Context, Div, FontWeight, IntoElement, div, prelude::*, px};
use leon_core::{Detail, Intent};
use leon_domain::{GhKind, GhRef, Issue};

use crate::drawer::drawer;
use crate::github::pr_icon;
use crate::icons::{Icon, icon};
use crate::pr_view::labels;
use crate::sprints::board_chips;
use crate::theme;
use crate::widgets::{empty, label};
use crate::workspace::Workspace;

impl Workspace {
    /// The details drawer, while an item is shown.
    pub(crate) fn render_detail(&mut self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let detail = self.snapshot.detail.clone()?;
        let item = detail.item();
        let input = self.gh.comment.clone();
        let body: AnyElement = match &detail {
            Detail::Loading(_) => empty("Reading from GitHub…"),
            Detail::Failed(_, error) => empty(format!("Could not read it: {error}")),
            Detail::Issue(issue) => self.issue_body(issue, cx).into_any_element(),
            Detail::PullRequest(pr) => self.pr_body(pr, &input, cx).into_any_element(),
        };
        // What Leon can do with it sits in the header, left of GitHub.
        let actions = match &detail {
            Detail::Issue(issue) => vec![self.issue_action(issue, cx)],
            Detail::PullRequest(pr) => Self::pr_actions(pr, cx),
            Detail::Loading(_) | Detail::Failed(..) => Vec::new(),
        };
        let glyph = match &detail {
            Detail::PullRequest(pr) => pr_icon(pr.state, 16.),
            _ => match item.kind {
                GhKind::Issue => icon(Icon::Issue, theme::fg_muted(), 16.),
                GhKind::PullRequest => icon(Icon::PullRequest, theme::fg_muted(), 16.),
            },
        };
        let url = web_url(&item);
        let header = div()
            .flex()
            .items_center()
            .gap_2()
            .child(glyph)
            .child(
                div()
                    .font_family(theme::MONO)
                    .text_size(px(12.5))
                    .text_color(theme::fg_muted())
                    .child(item.text()),
            )
            .child(div().flex_1())
            .children(actions)
            .when(item.kind == GhKind::PullRequest, |d| {
                d.child(copy_link(url.clone()))
            })
            .child(github_link(url));
        let scroll = div()
            .id("detail-body")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .pb_6()
            .child(
                div()
                    .max_w(px(theme::COLUMN))
                    .w_full()
                    .mx_auto()
                    .child(body),
            );
        Some(drawer(
            header,
            scroll,
            None,
            cx.listener(|this, _, _, _| this.core.send(Intent::ShowDetail(None))),
        ))
    }

    /// An issue: title, board fields, labels, assignees, the flow
    /// button, its text and its comments.
    fn issue_body(&self, issue: &Issue, cx: &mut Context<Self>) -> Div {
        let status = issue.status.as_ref().map(|s| {
            let color = self.status_color(s);
            div()
                .flex()
                .items_center()
                .gap(px(5.))
                .px(px(8.))
                .py(px(1.))
                .rounded_full()
                .border_1()
                .border_color(gpui::Rgba { a: 0.6, ..color })
                .bg(gpui::Rgba { a: 0.15, ..color })
                .text_size(px(11.5))
                .text_color(color)
                .child(div().size(px(7.)).rounded_full().bg(color))
                .child(s.name.clone())
        });
        let fields = div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap_2()
            .children(status)
            .children(board_chips(issue))
            .child(labels(&issue.labels));
        let item = GhRef::issue(&issue.repo, issue.number);
        div()
            .flex()
            .flex_col()
            .child(
                div()
                    .px_3()
                    .pt_4()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(
                        div()
                            .text_size(px(17.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(issue.title.clone()),
                    )
                    .child(fields)
                    .when(!issue.assignees.is_empty(), |d| {
                        d.child(
                            div()
                                .text_size(px(12.))
                                .text_color(theme::fg_muted())
                                .child(format!("Assigned to {}", issue.assignees.join(", "))),
                        )
                    }),
            )
            .child(label("Description"))
            .child(if issue.body.trim().is_empty() {
                empty("No description.")
            } else {
                div()
                    .mx_3()
                    .px_3()
                    .py_2()
                    .rounded_lg()
                    .border_1()
                    .border_color(theme::border())
                    .bg(theme::panel())
                    .text_size(px(13.))
                    .child(self.rich_markdown(
                        &format!("issue-body-{}", item.text()),
                        &issue.body,
                        cx,
                    ))
                    .into_any_element()
            })
            .child(self.comments(&issue.comments, item, &self.gh.comment, cx))
    }
}

/// "Open on GitHub", opening `url` in the browser.
pub(crate) fn github_link(url: String) -> AnyElement {
    header_button(
        "detail-on-github",
        Icon::External,
        "Open on GitHub",
        move |cx| {
            cx.open_url(&url);
        },
    )
}

/// "Copy link", putting a pull request's `url` on the clipboard.
fn copy_link(url: String) -> AnyElement {
    header_button("detail-copy-link", Icon::Copy, "Copy link", move |cx| {
        cx.write_to_clipboard(gpui::ClipboardItem::new_string(url.clone()));
    })
}

/// A quiet button of the details drawer's header.
fn header_button(
    id: &'static str,
    glyph: Icon,
    label: &'static str,
    click: impl Fn(&mut gpui::App) + 'static,
) -> AnyElement {
    div()
        .id(id)
        .flex()
        .flex_none()
        .items_center()
        .gap(px(5.))
        .px_2()
        .py(px(3.))
        .rounded_md()
        .text_size(px(12.))
        .text_color(theme::fg_muted())
        .cursor_pointer()
        .hover(|s| s.bg(theme::surface_2()).text_color(theme::fg()))
        .child(icon(glyph, theme::fg_muted(), 13.))
        .child(label)
        .on_click(move |_, _, cx| click(cx))
        .into_any_element()
}

/// The item's page on GitHub.
fn web_url(item: &GhRef) -> String {
    let path = match item.kind {
        GhKind::Issue => "issues",
        GhKind::PullRequest => "pull",
    };
    format!("https://github.com/{}/{path}/{}", item.repo, item.number)
}
