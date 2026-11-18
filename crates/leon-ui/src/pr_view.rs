//! The conversation's pull request (GH-1, GH-4..6): state, checks,
//! reviewers, comments; request a Copilot review, post a comment.

use gpui::{AnyElement, Context, Div, Entity, FontWeight, IntoElement, Rgba, div, prelude::*, px};
use gpui_component::input::{Textarea, TextareaState};
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
            .child(self.pr_body(&pr, &input, cx))
            .into_any_element()
    }

    /// A pull request, what matters to act on it: its state and branch, its
    /// checks summed up with only the failing ones listed, what can be done
    /// (ready or draft, Copilot review), its description, the review threads
    /// still open, and a box to comment (GH-1, GH-4..7).
    pub(crate) fn pr_body(
        &self,
        pr: &PullRequest,
        input: &Entity<TextareaState>,
        cx: &mut Context<Self>,
    ) -> Div {
        let item = GhRef::pull_request(&pr.repo, pr.number);
        let (color, word) = pr_state(pr.state);
        let live = matches!(pr.state, PrState::Open | PrState::Draft);
        let asked = copilot_asked(pr);
        let id = item.text();
        let actions = live.then(|| {
            let draft = pr.state == PrState::Draft;
            let (toggle, ready) = (item.clone(), item.clone());
            div()
                .px_3()
                .flex()
                .flex_wrap()
                .gap_2()
                .child(if draft {
                    primary(
                        "pr-ready",
                        "Ready for review",
                        cx.listener(move |this, _, _, _| {
                            this.core.send(Intent::SetDraft(ready.clone(), false));
                        }),
                    )
                } else {
                    button(
                        "pr-draft",
                        "Convert to draft",
                        cx.listener(move |this, _, _, _| {
                            this.core.send(Intent::SetDraft(toggle.clone(), true));
                        }),
                    )
                })
                .child({
                    let item = item.clone();
                    button(
                        "copilot-review",
                        if asked {
                            "Copilot review requested"
                        } else {
                            "Request Copilot review"
                        },
                        cx.listener(move |this, _, _, _| {
                            if !asked {
                                this.core.send(Intent::RequestCopilotReview(item.clone()));
                            }
                        }),
                    )
                    .when(asked, |b| b.opacity(0.6))
                })
        });
        let description = if pr.body.trim().is_empty() {
            empty("No description.")
        } else {
            div()
                .mx_3()
                .px_3()
                .py_2()
                .rounded_lg()
                .border_1()
                .border_color(theme::border())
                .bg(theme::window())
                .text_size(px(13.))
                .child(self.rich_markdown(&format!("{id}-body"), &pr.body, cx))
                .into_any_element()
        };
        let threads = pr.threads.iter().enumerate().map(|(t, thread)| {
            let place = match thread.line {
                Some(at) => format!("{}:{at}", thread.path),
                None => thread.path.clone(),
            };
            div()
                .mx_3()
                .rounded_lg()
                .border_1()
                .border_color(theme::border())
                .bg(theme::window())
                .overflow_hidden()
                .child(
                    div()
                        .px_3()
                        .py(px(6.))
                        .border_b_1()
                        .border_color(theme::border())
                        .bg(theme::panel())
                        .font_family(theme::MONO)
                        .text_size(px(11.5))
                        .text_color(theme::fg_muted())
                        .child(place),
                )
                .children(thread.comments.iter().enumerate().map(|(c, comment)| {
                    div()
                        .px_3()
                        .py_2()
                        .when(c > 0, |d| d.border_t_1().border_color(theme::border()))
                        .text_size(px(12.5))
                        .child(
                            div()
                                .text_size(px(11.5))
                                .text_color(theme::fg_faint())
                                .child(format!("{} · {}", comment.author, since(comment.at))),
                        )
                        .child(self.rich_markdown(
                            &format!("{id}-thread-{t}-{c}"),
                            &comment.body,
                            cx,
                        ))
                }))
        });
        let open_threads = pr.threads.len();
        div()
            .flex()
            .flex_col()
            .gap_1()
            .child(
                div()
                    .px_3()
                    .pt_3()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(crate::github::pr_icon(pr.state, 15.))
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(color)
                                    .child(word),
                            ),
                    )
                    .child(
                        div()
                            .text_size(px(15.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(pr.title.clone()),
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
            .child(label("Checks"))
            .child(checks_summary(pr))
            .children(actions.map(gpui::Styled::pt_2))
            .child(label("Description"))
            .child(description)
            .child(label(if open_threads == 0 {
                "Review threads".to_owned()
            } else {
                format!("Review threads · {open_threads} open")
            }))
            .when(open_threads == 0, |d| d.child(empty("No open thread.")))
            .child(div().flex().flex_col().gap_2().children(threads))
            .child(Self::comment_box(item, input, cx))
    }

    /// Comments, oldest first, then a box to post one on `item` (GH-6).
    pub(crate) fn comments(
        &self,
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
                .child(self.rich_markdown(&format!("{}-comment-{ix}", item.text()), &c.body, cx))
        });
        div()
            .flex()
            .flex_col()
            .child(label("Comments"))
            .when(comments.is_empty(), |d| d.child(empty("No comments")))
            .children(list)
            .child(Self::comment_box(item, input, cx))
    }

    /// A box to post a comment on `item` (GH-6).
    fn comment_box(item: GhRef, input: &Entity<TextareaState>, cx: &mut Context<Self>) -> Div {
        let input = input.clone();
        div()
            .pt_3()
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
            )))
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

/// The checks summed up — "All 12 passing", "2 of 12 failing" — then each
/// failing one, by name.
fn checks_summary(pr: &PullRequest) -> AnyElement {
    let total = pr.checks.len();
    let failing: Vec<&leon_domain::Check> = pr
        .checks
        .iter()
        .filter(|c| c.state == CheckState::Fail)
        .collect();
    let running = pr
        .checks
        .iter()
        .filter(|c| c.state == CheckState::Pending)
        .count();
    let (state, text) = match (total, failing.len(), running) {
        (0, _, _) => match pr.checks_state() {
            Some(state) => (state, "Checks summed up by GitHub".to_owned()),
            None => return empty("No checks."),
        },
        (_, 0, 0) => (CheckState::Pass, format!("All {total} passing")),
        (_, 0, r) => (CheckState::Pending, format!("{r} of {total} running")),
        (_, f, _) => (CheckState::Fail, format!("{f} of {total} failing")),
    };
    let glyph = |s: CheckState| match s {
        CheckState::Pass => Icon::Passed,
        CheckState::Fail => Icon::Failed,
        CheckState::Pending => Icon::Pending,
    };
    div()
        .mx_3()
        .flex()
        .flex_col()
        .gap(px(6.))
        .text_size(px(12.5))
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(icon(glyph(state), check_color(state), 14.))
                .child(text),
        )
        .children(failing.into_iter().map(|check| {
            div()
                .pl_6()
                .flex()
                .items_center()
                .gap_2()
                .text_color(theme::fg_muted())
                .child(icon(Icon::Failed, theme::err(), 12.))
                .child(check.name.clone())
        }))
        .into_any_element()
}
