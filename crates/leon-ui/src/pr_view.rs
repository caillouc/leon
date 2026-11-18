//! The conversation's pull request (GH-1, GH-4..6): state, checks,
//! reviewers, comments; request a Copilot review, post a comment.

use gpui::{AnyElement, Context, Div, Entity, IntoElement, Rgba, div, prelude::*, px};
use gpui_component::input::TextareaState;
use leon_domain::{CheckState, Comment, ConversationId, GhRef, Label, PrState, PullRequest};

use crate::gh_detail::github_link;
use crate::left::since;
use crate::sprints::tinted_chip;
use crate::theme;
use crate::widgets::empty;
use crate::workspace::Workspace;

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
    div().flex().flex_wrap().gap(px(5.)).children(
        labels
            .iter()
            .map(|l| tinted_chip(&l.name, theme::hex(&l.color).unwrap_or(theme::fg_faint()))),
    )
}

impl Workspace {
    /// The PR tab of the right pane: the conversation's pull request.
    pub(crate) fn render_pr(&mut self, id: ConversationId, cx: &mut Context<Self>) -> AnyElement {
        let Some(pr) = self.folder_pr(id).cloned() else {
            let branch = self
                .folder_git(id)
                .and_then(|g| g.branch.clone())
                .unwrap_or_else(|| "this branch".into());
            return empty(format!(
                "No pull request for {branch} yet. Ask the agent to open one."
            ));
        };
        let input = self.right.pr_comment.clone();
        // Its actions beside "Open on GitHub", as in the details drawer.
        let toolbar = div()
            .px_4()
            .pt_3()
            .flex()
            .flex_wrap()
            .items_center()
            .justify_end()
            .gap_2()
            .children(Self::pr_actions(&pr, cx))
            .child(github_link(pr.url.clone()));
        div()
            .id("right-pr")
            .flex_1()
            .overflow_y_scroll()
            .pb_4()
            .child(toolbar)
            .child(self.pr_body(&pr, &input, cx))
            .into_any_element()
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
                        .text_size(px(12.5))
                        .text_color(theme::fg_faint())
                        .child("No comments yet."),
                )
            })
            .children(list)
            .child(Self::comment_box(item, input, cx))
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
