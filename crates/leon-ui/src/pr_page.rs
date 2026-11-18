//! A pull request laid out like its page on GitHub (GH-1, GH-3..7): its
//! state and branches, the checks box, the description and each open review
//! thread as comment cards with the code they are on, people as coloured
//! avatars, and a box to comment.

use gpui::{AnyElement, Context, Div, Entity, FontWeight, IntoElement, Rgba, div, prelude::*, px};
use gpui_component::input::{Textarea, TextareaState};
use leon_core::Intent;
use leon_domain::{Check, CheckState, GhRef, PrState, PullRequest};

use crate::github::pr_glyph;
use crate::icons::{Icon, icon};
use crate::pr_view::{check_color, labels, pr_state};
use crate::theme;
use crate::widgets::{button, primary};
use crate::workspace::{Workspace, text_of};

/// `color` at `alpha`.
pub(crate) fn tint(color: Rgba, alpha: f32) -> Rgba {
    Rgba { a: alpha, ..color }
}

/// A person as a coloured disc with their initial; one colour per login.
pub(crate) fn avatar(login: &str, size: f32) -> Div {
    let color = theme::agent_color(login.bytes().map(usize::from).sum());
    let initial = login
        .chars()
        .find(char::is_ascii_alphanumeric)
        .map_or('?', |c| c.to_ascii_uppercase());
    div()
        .size(px(size))
        .flex_none()
        .rounded_full()
        .bg(color)
        .flex()
        .items_center()
        .justify_center()
        .text_size(px(size * 0.48))
        .font_weight(FontWeight::BOLD)
        .text_color(theme::on_accent())
        .child(initial.to_string())
}

/// The state as GitHub shows it: a filled pill with its icon.
fn state_pill(state: PrState) -> Div {
    let (color, word) = pr_state(state);
    div()
        .flex()
        .flex_none()
        .items_center()
        .gap(px(5.))
        .px(px(10.))
        .py(px(3.))
        .rounded_full()
        .bg(color)
        .text_size(px(12.5))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(theme::on_accent())
        .child(icon(pr_glyph(state), theme::on_accent(), 14.))
        .child(word)
}

/// A branch name, in code style and the accent colour.
fn branch(name: &str) -> Div {
    div()
        .px(px(6.))
        .py(px(1.))
        .rounded(px(6.))
        .bg(theme::accent_soft())
        .font_family(theme::MONO)
        .text_size(px(11.5))
        .text_color(theme::accent())
        .child(name.to_owned())
}

/// A section title with an optional count pill in `color`.
fn heading(text: &str, count: Option<(String, Rgba)>) -> Div {
    div()
        .flex()
        .items_center()
        .gap_2()
        .child(
            div()
                .text_size(px(14.))
                .font_weight(FontWeight::SEMIBOLD)
                .child(text.to_owned()),
        )
        .children(count.map(|(text, color)| {
            div()
                .px(px(8.))
                .rounded_full()
                .border_1()
                .border_color(tint(color, 0.6))
                .bg(tint(color, 0.15))
                .text_size(px(11.5))
                .text_color(color)
                .child(text)
        }))
}

/// Title and number, state, who wants to merge what into what, labels.
pub(crate) fn pr_header(pr: &PullRequest) -> Div {
    let verb = match pr.state {
        PrState::Open | PrState::Draft => "wants to merge into",
        PrState::Merged => "merged into",
        PrState::Closed => "wanted to merge into",
    };
    div()
        .flex()
        .flex_col()
        .gap_3()
        .child(
            div()
                .flex()
                .flex_wrap()
                .items_baseline()
                .gap_2()
                .text_size(px(21.))
                .child(
                    div()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(pr.title.clone()),
                )
                .child(
                    div()
                        .text_color(theme::fg_faint())
                        .child(format!("#{}", pr.number)),
                ),
        )
        .child(
            div()
                .flex()
                .flex_wrap()
                .items_center()
                .gap_2()
                .text_size(px(13.))
                .text_color(theme::fg_muted())
                .child(state_pill(pr.state))
                .when(!pr.author.is_empty(), |d| {
                    d.child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme::fg())
                            .child(pr.author.clone()),
                    )
                })
                .child(verb)
                .child(branch(&pr.base))
                .child("from")
                .child(branch(&pr.branch)),
        )
        .when(!pr.labels.is_empty(), |d| d.child(labels(&pr.labels)))
}

/// The checks as GitHub's merge box sums them up: a coloured header that
/// says how they went, then the failing and running ones by name.
pub(crate) fn checks_card(pr: &PullRequest) -> AnyElement {
    let count = |state| pr.checks.iter().filter(|c| c.state == state).count();
    let (failing, running, passing) = (
        count(CheckState::Fail),
        count(CheckState::Pending),
        count(CheckState::Pass),
    );
    let Some(state) = pr.checks_state() else {
        return div()
            .text_size(px(12.5))
            .text_color(theme::fg_faint())
            .child("No checks on this pull request.")
            .into_any_element();
    };
    let title = match state {
        CheckState::Pass => "All checks have passed",
        CheckState::Fail => "Some checks were not successful",
        CheckState::Pending => "Some checks haven't completed yet",
    };
    let parts: Vec<String> = [
        (failing, "failing"),
        (running, "in progress"),
        (passing, "successful"),
    ]
    .into_iter()
    .filter(|(n, _)| *n > 0)
    .map(|(n, word)| format!("{n} {word}"))
    .collect();
    let detail = if pr.checks.is_empty() {
        "As GitHub sums them up".to_owned()
    } else {
        format!(
            "{} {}",
            parts.join(", "),
            if pr.checks.len() == 1 {
                "check"
            } else {
                "checks"
            }
        )
    };
    let color = check_color(state);
    let shown: Vec<&Check> = pr
        .checks
        .iter()
        .filter(|c| c.state != CheckState::Pass)
        .collect();
    div()
        .rounded_lg()
        .border_1()
        .border_color(tint(color, 0.55))
        .bg(theme::panel())
        .overflow_hidden()
        .child(
            div()
                .px_4()
                .py_3()
                .flex()
                .items_center()
                .gap_3()
                .bg(tint(color, 0.08))
                .child(
                    div()
                        .size(px(30.))
                        .flex_none()
                        .rounded_full()
                        .bg(tint(color, 0.2))
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(icon(check_glyph(state), color, 17.)),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .child(
                            div()
                                .text_size(px(13.5))
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(title),
                        )
                        .child(
                            div()
                                .text_size(px(12.))
                                .text_color(theme::fg_muted())
                                .child(detail),
                        ),
                ),
        )
        .children(shown.into_iter().map(|check| {
            div()
                .px_4()
                .py(px(7.))
                .border_t_1()
                .border_color(theme::border())
                .flex()
                .items_center()
                .gap_2()
                .text_size(px(12.5))
                .child(icon(
                    check_glyph(check.state),
                    check_color(check.state),
                    14.,
                ))
                .child(div().flex_1().min_w_0().child(check.name.clone()))
                .child(
                    div()
                        .text_size(px(11.5))
                        .text_color(theme::fg_faint())
                        .child(if check.state == CheckState::Fail {
                            "Failing"
                        } else {
                            "In progress"
                        }),
                )
        }))
        .into_any_element()
}

fn check_glyph(state: CheckState) -> Icon {
    match state {
        CheckState::Pass => Icon::Passed,
        CheckState::Fail => Icon::Failed,
        CheckState::Pending => Icon::Pending,
    }
}

impl Workspace {
    /// What can be done with a pull request from Leon: ready or draft, a
    /// Copilot review. Shown next to "Open on GitHub".
    pub(crate) fn pr_actions(pr: &PullRequest, cx: &mut Context<Self>) -> Vec<AnyElement> {
        if !matches!(pr.state, PrState::Open | PrState::Draft) {
            return Vec::new();
        }
        let item = GhRef::pull_request(&pr.repo, pr.number);
        let toggle = item.clone();
        let draft = if pr.state == PrState::Draft {
            primary(
                "pr-ready",
                "Ready for review",
                cx.listener(move |this, _, _, _| {
                    this.core.send(Intent::SetDraft(toggle.clone(), false));
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
        };
        let asked = pr
            .reviewers
            .iter()
            .any(|r| r.to_lowercase().contains("copilot"));
        let copilot = button(
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
        .when(asked, |b| b.opacity(0.6));
        vec![draft.into_any_element(), copilot.into_any_element()]
    }

    /// A comment as on GitHub's timeline: the author's avatar beside a card
    /// whose header names them.
    pub(crate) fn comment_card(
        &self,
        id: &str,
        author: &str,
        what: &str,
        body: &str,
        cx: &mut Context<Self>,
    ) -> Div {
        let who = if author.is_empty() { "Someone" } else { author };
        div()
            .flex()
            .items_start()
            .gap_3()
            .child(avatar(who, 32.))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .rounded_lg()
                    .border_1()
                    .border_color(theme::border_strong())
                    .bg(theme::panel())
                    .overflow_hidden()
                    .child(
                        div()
                            .px_3()
                            .py(px(7.))
                            .flex()
                            .items_center()
                            .gap(px(5.))
                            .bg(theme::surface_2())
                            .border_b_1()
                            .border_color(theme::border())
                            .text_size(px(12.5))
                            .child(
                                div()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(who.to_owned()),
                            )
                            .child(div().text_color(theme::fg_muted()).child(what.to_owned())),
                    )
                    .child(div().px_3().py_2().text_size(px(13.)).child(
                        if body.trim().is_empty() {
                            div()
                                .text_color(theme::fg_faint())
                                .child("No description provided.")
                                .into_any_element()
                        } else {
                            self.rich_markdown(id, body, cx)
                        },
                    )),
            )
    }

    /// A card to post a comment on `item` (GH-6), its button at the right.
    pub(crate) fn comment_box(
        item: GhRef,
        input: &Entity<TextareaState>,
        cx: &mut Context<Self>,
    ) -> Div {
        let input = input.clone();
        div()
            .rounded_lg()
            .border_1()
            .border_color(theme::border_strong())
            .bg(theme::panel())
            .overflow_hidden()
            .child(
                div()
                    .px_3()
                    .py(px(7.))
                    .bg(theme::surface_2())
                    .border_b_1()
                    .border_color(theme::border())
                    .text_size(px(12.5))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("Add a comment"),
            )
            .child(div().p_2().child(Textarea::new(&input).appearance(false)))
            .child(div().px_3().pb_3().flex().justify_end().child(primary(
                "post-comment",
                "Comment",
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

    /// A pull request, as on GitHub: header, checks, description, the
    /// review threads still open, a box to comment (GH-1, GH-3..7).
    pub(crate) fn pr_body(
        &self,
        pr: &PullRequest,
        input: &Entity<TextareaState>,
        cx: &mut Context<Self>,
    ) -> Div {
        let item = GhRef::pull_request(&pr.repo, pr.number);
        let id = item.text();
        let open = pr.threads.len();
        let count = (open > 0).then(|| (format!("{open} unresolved"), theme::warn()));
        div()
            .px_4()
            .pt_4()
            .flex()
            .flex_col()
            .gap_4()
            .child(pr_header(pr))
            .child(checks_card(pr))
            .child(self.comment_card(
                &format!("{id}-body"),
                &pr.author,
                "opened this pull request",
                &pr.body,
                cx,
            ))
            .child(heading("Review threads", count))
            .when(open == 0, |d| {
                d.child(
                    div()
                        .text_size(px(12.5))
                        .text_color(theme::fg_faint())
                        .child("No open thread."),
                )
            })
            .children(
                pr.threads.iter().enumerate().map(|(t, thread)| {
                    self.thread_card(&format!("{id}-thread-{t}"), pr, thread, cx)
                }),
            )
            .child(Self::comment_box(item, input, cx))
    }
}
