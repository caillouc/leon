//! The conversation's pull request (GH-1, GH-4..6): state, checks,
//! reviewers, comments; request a Copilot review, post a comment.

use gpui::{
    AnyElement, Context, Div, Entity, FontWeight, IntoElement, Rgba, SharedString, div, prelude::*,
    px,
};
use gpui_component::input::{Textarea, TextareaState};
use gpui_component::text::TextView;
use leon_core::Intent;
use leon_domain::{CheckState, Comment, ConversationId, GhRef, Label, PrState, PullRequest};

use crate::icons::{Icon, icon};
use crate::left::since;
use crate::theme;
use crate::widgets::{button, empty, label, primary};
use crate::workspace::{Workspace, text_of};

/// Colour and word of a pull request state.
pub(crate) fn pr_state(state: PrState) -> (Rgba, &'static str) {
    match state {
        PrState::Open => (theme::ok(), "Open"),
        PrState::Draft => (theme::fg_muted(), "Draft"),
        PrState::Merged => (theme::merged(), "Merged"),
        PrState::Closed => (theme::err(), "Closed"),
    }
}

/// Colour of a check state.
pub(crate) fn check_color(state: CheckState) -> Rgba {
    match state {
        CheckState::Pass => theme::ok(),
        CheckState::Fail => theme::err(),
        CheckState::Pending => theme::warn(),
    }
}

/// Labels as small pills in GitHub's own colors (GH-4), readable on the
/// dark theme: tinted background, colored border, light text.
pub(crate) fn labels(labels: &[Label]) -> Div {
    div()
        .flex()
        .flex_wrap()
        .gap(px(5.))
        .children(labels.iter().map(|l| {
            let color = theme::hex(&l.color).unwrap_or(theme::fg_faint());
            div()
                .px_2()
                .rounded_full()
                .border_1()
                .border_color(gpui::Rgba { a: 0.7, ..color })
                .bg(gpui::Rgba { a: 0.18, ..color })
                .text_size(px(11.))
                .text_color(theme::fg())
                .child(l.name.clone())
        }))
}

/// Whether Copilot is already among the reviewers. ⚠️ Copilot's reviewer
/// login varies by API; any login containing "copilot" counts.
fn copilot_asked(pr: &PullRequest) -> bool {
    pr.reviewers
        .iter()
        .any(|r| r.to_lowercase().contains("copilot"))
}

impl Workspace {
    /// The PR tab of the right pane: the conversation's pull request.
    pub(crate) fn render_pr(&mut self, id: ConversationId, cx: &mut Context<Self>) -> AnyElement {
        let Some(pr) = self.snapshot.prs.get(&id).cloned() else {
            let branch = self
                .snapshot
                .git
                .get(&id)
                .and_then(|g| g.branch.clone())
                .unwrap_or_else(|| "this branch".into());
            return empty(format!(
                "No pull request for {branch} yet. Ask the agent to open one."
            ));
        };
        let input = self.right.pr_comment.clone();
        div()
            .id("right-pr")
            .flex_1()
            .overflow_y_scroll()
            .pb_4()
            .child(Self::pr_body(&pr, &input, cx))
            .into_any_element()
    }

    /// A pull request: state, title, branch, labels, checks, reviewers,
    /// comments, and the actions on it (GH-1, GH-4..6). `input` is the
    /// comment box to use.
    pub(crate) fn pr_body(
        pr: &PullRequest,
        input: &Entity<TextareaState>,
        cx: &mut Context<Self>,
    ) -> Div {
        let item = GhRef::pull_request(&pr.repo, pr.number);
        let (color, word) = pr_state(pr.state);
        let live = matches!(pr.state, PrState::Open | PrState::Draft);
        let passing = pr
            .checks
            .iter()
            .filter(|c| c.state == CheckState::Pass)
            .count();
        let asked = copilot_asked(pr);
        let checks = div()
            .mx_3()
            .flex()
            .flex_col()
            .rounded_lg()
            .border_1()
            .border_color(theme::border())
            .bg(theme::window())
            .children(pr.checks.iter().enumerate().map(|(ix, check)| {
                let glyph = match check.state {
                    CheckState::Pass => Icon::Passed,
                    CheckState::Fail => Icon::Failed,
                    CheckState::Pending => Icon::Pending,
                };
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_3()
                    .py(px(7.))
                    .when(ix > 0, |d| d.border_t_1().border_color(theme::border()))
                    .text_size(px(12.5))
                    .child(icon(glyph, check_color(check.state), 14.))
                    .child(check.name.clone())
            }));
        let request = {
            let item = item.clone();
            cx.listener(move |this, _, _, _| {
                if !asked {
                    this.core.send(Intent::RequestCopilotReview(item.clone()));
                }
            })
        };
        div()
            .flex()
            .flex_col()
            .child(
                div()
                    .px_3()
                    .pt_3()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        div()
                            .px_2()
                            .py(px(1.))
                            .rounded_full()
                            .bg(theme::surface_2())
                            .text_size(px(11.5))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(color)
                            .self_start()
                            .child(word),
                    )
                    .child(
                        div()
                            .text_size(px(14.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(format!("{} #{}", pr.title, pr.number)),
                    )
                    .child(
                        div()
                            .font_family(theme::MONO)
                            .text_size(px(11.5))
                            .text_color(theme::fg_muted())
                            .child(format!("{} → {} · {}", pr.branch, pr.base, pr.repo)),
                    )
                    .child(labels(&pr.labels)),
            )
            .child(label(format!(
                "Checks {passing}/{} passing",
                pr.checks.len()
            )))
            .child(if pr.checks.is_empty() {
                empty("No checks")
            } else {
                checks.into_any_element()
            })
            .child(label("Reviewers"))
            .children(
                pr.reviewers
                    .iter()
                    .map(|r| div().px_3().text_size(px(12.5)).child(r.clone())),
            )
            .when(pr.reviewers.is_empty(), |d| {
                d.child(empty("No reviewers yet"))
            })
            .when(live, |d| {
                d.child(
                    div().px_3().pt_2().flex().child(
                        button(
                            "copilot-review",
                            if asked {
                                "Copilot review requested"
                            } else {
                                "Request Copilot review"
                            },
                            request,
                        )
                        .when(asked, |b| b.opacity(0.6)),
                    ),
                )
            })
            .child(Self::comments(&pr.comments, item, input, cx))
    }

    /// Comments, oldest first, then a box to post one on `item` (GH-6).
    pub(crate) fn comments(
        comments: &[Comment],
        item: GhRef,
        input: &Entity<TextareaState>,
        cx: &mut Context<Self>,
    ) -> Div {
        let list = comments.iter().enumerate().map(|(ix, c)| {
            div()
                .mx_3()
                .mb_2()
                .px_3()
                .py_2()
                .rounded_lg()
                .border_1()
                .border_color(theme::border())
                .bg(theme::window())
                .text_size(px(12.5))
                .child(
                    div()
                        .text_size(px(11.5))
                        .text_color(theme::fg_faint())
                        .child(format!("{} · {}", c.author, since(c.at))),
                )
                .child(TextView::markdown(
                    SharedString::from(format!("comment-{ix}")),
                    SharedString::from(c.body.clone()),
                ))
        });
        let input = input.clone();
        div()
            .flex()
            .flex_col()
            .child(label("Comments"))
            .when(comments.is_empty(), |d| d.child(empty("No comments")))
            .children(list)
            .child(
                div()
                    .px_3()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(Textarea::new(&input))
                    .child(div().flex().child(primary(
                        "post-comment",
                        "Post comment",
                        cx.listener(move |this, _, window, cx| {
                            let text = text_of(&input, cx);
                            if text.trim().is_empty() {
                                return;
                            }
                            this.core
                                .send(Intent::Comment(item.clone(), text.trim().to_owned()));
                            input.update(cx, |state, cx| state.set_value("", window, cx));
                        }),
                    ))),
            )
    }
}

/// A pull request's badge in the left pane (NAV-6): state colour, number,
/// and a dot for its checks while it is open.
pub(crate) fn pr_badge(pr: &PullRequest) -> AnyElement {
    let (color, _) = pr_state(pr.state);
    let live = matches!(pr.state, PrState::Open | PrState::Draft);
    div()
        .flex()
        .flex_none()
        .items_center()
        .gap(px(3.))
        .px(px(5.))
        .rounded(px(4.))
        .border_1()
        .border_color(theme::border())
        .font_family(theme::MONO)
        .text_size(px(10.5))
        .text_color(color)
        .child(format!("#{}", pr.number))
        .children(
            pr.checks_state()
                .filter(|_| live)
                .map(|s| div().size(px(6.)).rounded_full().bg(check_color(s))),
        )
        .into_any_element()
}
