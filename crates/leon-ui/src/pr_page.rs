//! A pull request laid out like its page on GitHub (GH-1, GH-3..7): its
//! state and branches, the checks box, the description and each open review
//! thread as comment cards with the code they are on, people as coloured
//! avatars, and a box to comment.

use gpui::{AnyElement, Context, Div, Entity, FontWeight, IntoElement, Rgba, div, prelude::*};
use gpui_component::input::{Textarea, TextareaState};
use leon_core::{Acting, Intent};
use leon_domain::{Check, CheckState, Comment, GhRef, PrState, PullRequest};

use crate::github::pr_glyph;
use crate::icons::{Icon, icon};
use crate::left::since;
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
    let color = theme::name_color(login);
    let initial = login
        .chars()
        .find(char::is_ascii_alphanumeric)
        .map_or('?', |c| c.to_ascii_uppercase());
    div()
        .size(crate::theme::rem(size))
        .flex_none()
        .rounded_full()
        .bg(color)
        .flex()
        .items_center()
        .justify_center()
        .text_size(crate::theme::rem(size * 0.48))
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
        .gap(crate::theme::rem(5.))
        .px(crate::theme::rem(10.))
        .py(crate::theme::rem(3.))
        .rounded_full()
        .bg(color)
        .text_size(crate::theme::rem(12.5))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(theme::on_accent())
        .child(icon(pr_glyph(state), theme::on_accent(), 14.))
        .child(word)
}

/// A branch name, in code style and the accent colour.
fn branch(name: &str) -> Div {
    div()
        .px(crate::theme::rem(6.))
        .py(crate::theme::rem(1.))
        .rounded(crate::theme::rem(6.))
        .bg(theme::accent_soft())
        .font_family(theme::MONO)
        .text_size(crate::theme::rem(11.5))
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
                .text_size(crate::theme::rem(14.))
                .font_weight(FontWeight::SEMIBOLD)
                .child(text.to_owned()),
        )
        .children(count.map(|(text, color)| {
            div()
                .px(crate::theme::rem(8.))
                .rounded_full()
                .border_1()
                .border_color(tint(color, 0.6))
                .bg(tint(color, 0.15))
                .text_size(crate::theme::rem(11.5))
                .text_color(color)
                .child(text)
        }))
}

/// Title and number; under it state, who wants to merge what into what,
/// and its labels on the right.
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
                .text_size(crate::theme::rem(21.))
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
                .items_center()
                .gap_3()
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .gap_2()
                        .text_size(crate::theme::rem(13.))
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
                .when(!pr.labels.is_empty(), |d| {
                    d.child(
                        labels(&pr.labels)
                            .max_w(crate::theme::rem(280.))
                            .justify_end(),
                    )
                }),
        )
}

/// The checks as GitHub's merge box sums them up: a coloured header that
/// says how they went, then the failing and running ones by name.
pub(crate) fn checks_card(pr: &PullRequest) -> AnyElement {
    let count = |state| pr.checks.iter().filter(|c| c.state == state).count();
    let (failing, running, passing, skipped) = (
        count(CheckState::Fail),
        count(CheckState::Pending),
        count(CheckState::Pass),
        count(CheckState::Skipped),
    );
    let Some(state) = pr.checks_state() else {
        return div()
            .text_size(crate::theme::rem(12.5))
            .text_color(theme::fg_faint())
            .child("No checks on this pull request.")
            .into_any_element();
    };
    let title = match state {
        CheckState::Pass => "All checks have passed",
        CheckState::Skipped => "All checks were skipped",
        CheckState::Fail => "Some checks were not successful",
        CheckState::Pending => "Some checks haven't completed yet",
    };
    let parts: Vec<String> = [
        (failing, "failing"),
        (running, "in progress"),
        (passing, "successful"),
        (skipped, "skipped"),
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
        .filter(|c| matches!(c.state, CheckState::Fail | CheckState::Pending))
        .collect();
    merge_box(color)
        .child(merge_header(check_glyph(state), color, title, detail))
        .children(shown.into_iter().enumerate().map(|(ix, check)| {
            check_row(ix, check)
                .px_4()
                .border_t_1()
                .border_color(theme::border())
        }))
        .into_any_element()
}

/// GitHub's merge box for a branch that conflicts with its base: shown
/// while the pull request is open (GH-3).
pub(crate) fn conflicts_card(pr: &PullRequest) -> Option<AnyElement> {
    let open = matches!(pr.state, PrState::Open | PrState::Draft);
    (pr.conflicts && open).then(|| {
        let color = theme::err();
        merge_box(color)
            .child(merge_header(
                Icon::Failed,
                color,
                "This branch has conflicts that must be resolved",
                format!("{} conflicts with {}", pr.branch, pr.base),
            ))
            .into_any_element()
    })
}

/// A box of GitHub's merge area, its border in `color`.
fn merge_box(color: Rgba) -> Div {
    div()
        .rounded_lg()
        .border_1()
        .border_color(tint(color, 0.55))
        .bg(theme::panel())
        .overflow_hidden()
}

/// A merge box's header as GitHub draws it: its icon in a circle, a title
/// and a line under it, on a tint of `color`.
fn merge_header(
    glyph: Icon,
    color: Rgba,
    title: impl IntoElement,
    detail: impl IntoElement,
) -> Div {
    div()
        .px_4()
        .py_3()
        .flex()
        .items_center()
        .gap_3()
        .bg(tint(color, 0.08))
        .child(
            div()
                .size(crate::theme::rem(30.))
                .flex_none()
                .rounded_full()
                .bg(tint(color, 0.2))
                .flex()
                .items_center()
                .justify_center()
                .child(icon(glyph, color, 17.)),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .child(
                    div()
                        .text_size(crate::theme::rem(13.5))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(title),
                )
                .child(
                    div()
                        .text_size(crate::theme::rem(12.))
                        .text_color(theme::fg_muted())
                        .child(detail),
                ),
        )
}

/// A check that did not pass: its mark, its name and its state; when GitHub
/// links its run, a click on the state opens it (an Actions job's log).
pub(crate) fn check_row(ix: usize, check: &Check) -> Div {
    let state = if check.state == CheckState::Fail {
        "Failing"
    } else {
        "In progress"
    };
    // Only "Failing ↗" opens the job: the rest of the row belongs to the
    // card around it.
    let link = div()
        .id(gpui::SharedString::from(format!(
            "check-{ix}-{}",
            check.name
        )))
        .flex_none()
        .flex()
        .items_center()
        .gap_1()
        .text_size(crate::theme::rem(11.5))
        .text_color(theme::fg_faint())
        .child(state)
        .when_some(check.url.clone(), |d, url| {
            d.cursor_pointer()
                .hover(|s| s.text_color(theme::fg()))
                .child(icon(Icon::External, theme::fg_muted(), 12.))
                .on_click(move |_, _, cx| {
                    // Not the card around it, which opens the drawer.
                    cx.stop_propagation();
                    cx.open_url(&url);
                })
        });
    div()
        .py(crate::theme::rem(7.))
        .flex()
        .items_center()
        .gap_2()
        .text_size(crate::theme::rem(12.5))
        .child(icon(
            check_glyph(check.state),
            check_color(check.state),
            14.,
        ))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .child(check.name.clone()),
        )
        .child(link)
}

pub(crate) fn check_glyph(state: CheckState) -> Icon {
    match state {
        CheckState::Pass => Icon::Passed,
        CheckState::Skipped => Icon::Skipped,
        CheckState::Fail => Icon::Failed,
        CheckState::Pending => Icon::Pending,
    }
}

impl Workspace {
    /// What can be done with a pull request from Leon: a Copilot review,
    /// ready or draft. Shown next to "Open on GitHub"; a click shows the
    /// action under way at once, its notice says how it went.
    pub(crate) fn pr_actions(&self, pr: &PullRequest, cx: &mut Context<Self>) -> Vec<AnyElement> {
        if !matches!(pr.state, PrState::Open | PrState::Draft) {
            return Vec::new();
        }
        let item = GhRef::pull_request(&pr.repo, pr.number);
        let acting = &self.snapshot.acting;
        let toggle = item.clone();
        let drafting = acting.contains(&Acting::Draft(item.clone()));
        let draft = if pr.state == PrState::Draft {
            primary(
                "pr-ready",
                if drafting {
                    "Marking ready…"
                } else {
                    "Ready for review"
                },
                cx.listener(move |this, _, _, _| {
                    this.act(Intent::SetDraft(toggle.clone(), false));
                }),
            )
        } else {
            button(
                "pr-draft",
                if drafting {
                    "Converting to draft…"
                } else {
                    "Convert to draft"
                },
                cx.listener(move |this, _, _, _| {
                    this.act(Intent::SetDraft(toggle.clone(), true));
                }),
            )
        }
        .when(drafting, |b| b.opacity(0.6));
        let asked = pr
            .reviewers
            .iter()
            .any(|r| r.to_lowercase().contains("copilot"));
        let asking = acting.contains(&Acting::CopilotReview(item.clone()));
        let copilot = button(
            "copilot-review",
            match (asking, asked) {
                (true, _) => "Requesting a review…",
                (false, true) => "Copilot review requested",
                (false, false) => "Request Copilot review",
            },
            cx.listener(move |this, _, _, _| {
                if !asked {
                    this.act(Intent::RequestCopilotReview(item.clone()));
                }
            }),
        )
        .when(asked || asking, |b| b.opacity(0.6));
        vec![copilot.into_any_element(), draft.into_any_element()]
    }

    /// Sends a GitHub action, unless the same one is under way: a second
    /// click does nothing (GH-5..8).
    pub(crate) fn act(&self, intent: Intent) {
        let under_way = match &intent {
            Intent::SetDraft(item, _) => Some(Acting::Draft(item.clone())),
            Intent::RequestCopilotReview(item) => Some(Acting::CopilotReview(item.clone())),
            Intent::Comment(item, _) => Some(Acting::Comment(item.clone())),
            Intent::ReplyToThread { thread, .. } => Some(Acting::Reply(thread.clone())),
            Intent::ResolveThread { thread, .. } => Some(Acting::Resolve(thread.clone())),
            _ => None,
        };
        if under_way.is_none_or(|a| !self.snapshot.acting.contains(&a)) {
            self.core.send(intent);
        }
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
                            .py(crate::theme::rem(7.))
                            .flex()
                            .items_center()
                            .gap(crate::theme::rem(5.))
                            .bg(theme::surface_2())
                            .border_b_1()
                            .border_color(theme::border())
                            .text_size(crate::theme::rem(12.5))
                            .child(
                                div()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(who.to_owned()),
                            )
                            .child(div().text_color(theme::fg_muted()).child(what.to_owned())),
                    )
                    .child(div().px_3().py_2().text_size(crate::theme::rem(13.)).child(
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

    /// Comments, oldest first, as cards with their authors, then a box to
    /// post one on `item` (GH-6).
    pub(crate) fn comments(
        &self,
        comments: &[Comment],
        item: GhRef,
        input: &Entity<TextareaState>,
        cx: &mut Context<Self>,
    ) -> Div {
        let list = comments.iter().enumerate().map(|(ix, c)| {
            self.comment_card(
                &format!("{}-comment-{ix}", item.text()),
                &c.author,
                &format!("commented {}", since(c.at)),
                &c.body,
                cx,
            )
        });
        div()
            .px_4()
            .pt_4()
            .flex()
            .flex_col()
            .gap_4()
            .when(comments.is_empty(), |d| {
                d.child(
                    div()
                        .text_size(crate::theme::rem(12.5))
                        .text_color(theme::fg_faint())
                        .child("No comments yet."),
                )
            })
            .children(list)
            .child(self.comment_box(item, input, cx))
    }

    /// A card to post a comment on `item` (GH-6), its button at the right.
    pub(crate) fn comment_box(
        &self,
        item: GhRef,
        input: &Entity<TextareaState>,
        cx: &mut Context<Self>,
    ) -> Div {
        let input = input.clone();
        let posting = self
            .snapshot
            .acting
            .contains(&Acting::Comment(item.clone()));
        div()
            .rounded_lg()
            .border_1()
            .border_color(theme::border_strong())
            .bg(theme::panel())
            .overflow_hidden()
            .child(
                div()
                    .px_3()
                    .py(crate::theme::rem(7.))
                    .bg(theme::surface_2())
                    .border_b_1()
                    .border_color(theme::border())
                    .text_size(crate::theme::rem(12.5))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("Add a comment"),
            )
            .child(div().p_2().child(Textarea::new(&input).appearance(false)))
            .child(
                div().px_3().pb_3().flex().justify_end().child(
                    primary(
                        "post-comment",
                        if posting { "Posting…" } else { "Comment" },
                        cx.listener(move |this, _, window, cx| {
                            let text = text_of(&input, cx);
                            if text.trim().is_empty() {
                                return;
                            }
                            this.act(Intent::Comment(item.clone(), text.trim().to_owned()));
                            input.update(cx, |state, cx| state.set_value("", window, cx));
                        }),
                    )
                    .when(posting, |b| b.opacity(0.6)),
                ),
            )
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
            .children(conflicts_card(pr))
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
                        .text_size(crate::theme::rem(12.5))
                        .text_color(theme::fg_faint())
                        .child("No open thread."),
                )
            })
            .children(
                pr.threads.iter().enumerate().map(|(t, thread)| {
                    self.thread_card(&format!("{id}-thread-{t}"), pr, thread, cx)
                }),
            )
            .child(self.comment_box(item, input, cx))
    }
}
