//! The Info tab's pull requests and issues (CTX-1, CTX-2), each a card that
//! opens in Leon's details drawer first, GitHub from there. A pull request
//! stands out: its state's colour along its left side, its state, its title
//! in bold, its lines and its checks that did not pass; a merged or closed
//! one, its work done, folds into one small line. An issue stays plain,
//! its icon in its board column's colour as in the issue list.

use gpui::{AnyElement, Context, Div, FontWeight, IntoElement, Stateful, div, prelude::*};
use leon_core::{FlowView, Intent};
use leon_domain::{CheckState, GhRef, PrState};

use crate::diff_rows::counts;
use crate::github::pr_icon;
use crate::icons::{Icon, icon};
use crate::pr_view::{labels, pr_state};
use crate::theme;
use crate::workspace::Workspace;

impl Workspace {
    /// The flow's pull requests, one card each.
    pub(crate) fn pr_cards(&self, view: &FlowView, cx: &mut Context<Self>) -> Vec<AnyElement> {
        self.snapshot
            .prs_of(view)
            .into_iter()
            .enumerate()
            .map(|(ix, pr)| {
                let item = GhRef::pull_request(&pr.repo, pr.number);
                if matches!(pr.state, PrState::Merged | PrState::Closed) {
                    return done_line(ix, pr, &item, cx);
                }
                let (color, state) = pr_state(pr.state);
                let failing = pr.checks.iter().filter(|c| c.state != CheckState::Pass);
                card(("info-pr", ix), &item, cx)
                    .relative()
                    .overflow_hidden()
                    .pl(crate::theme::rem(15.))
                    .child(
                        div()
                            .absolute()
                            .left_0()
                            .top_0()
                            .bottom_0()
                            .w(crate::theme::rem(3.))
                            .bg(color),
                    )
                    .child(
                        header(pr_icon(pr.state, 14.), "Pull request", &item).child(
                            div()
                                .flex_none()
                                .px(crate::theme::rem(6.))
                                .rounded(crate::theme::rem(4.))
                                .border_1()
                                .border_color(color)
                                .text_size(crate::theme::rem(11.))
                                .text_color(color)
                                .child(state),
                        ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                title(pr.title.clone())
                                    .text_size(crate::theme::rem(13.))
                                    .font_weight(FontWeight::SEMIBOLD),
                            )
                            .children(pr.lines.map(|(added, removed)| counts(added, removed))),
                    )
                    .when(!pr.labels.is_empty(), |d| d.child(labels(&pr.labels)))
                    // Conflicts with its base, as GitHub says (GH-3).
                    .when(
                        pr.conflicts && matches!(pr.state, PrState::Open | PrState::Draft),
                        |d| {
                            d.child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .text_size(crate::theme::rem(12.5))
                                    .text_color(theme::err())
                                    .child(icon(Icon::Failed, theme::err(), 14.))
                                    .child(format!("Conflicts with {}", pr.base)),
                            )
                        },
                    )
                    // Each check that did not pass opens its run.
                    .children(
                        failing
                            .enumerate()
                            .map(|(k, c)| crate::pr_page::check_row(ix * 100 + k, c)),
                    )
                    .into_any_element()
            })
            .collect()
    }

    /// The issues the flow works on, one card each, with their title and
    /// labels when known (an issue of mine or one linked to the flow is).
    pub(crate) fn issue_cards(&self, view: &FlowView, cx: &mut Context<Self>) -> Vec<AnyElement> {
        view.issues
            .iter()
            .enumerate()
            .map(|(ix, item)| {
                let known = self
                    .snapshot
                    .my_issues
                    .iter()
                    .find(|i| i.repo == item.repo && i.number == item.number)
                    .or_else(|| self.snapshot.linked_issues.get(item));
                card(("info-issue", ix), item, cx)
                    .child(header(
                        icon(
                            Icon::Issue,
                            self.issue_color(known.and_then(|i| i.status.as_ref())),
                            14.,
                        ),
                        "Issue",
                        item,
                    ))
                    .children(
                        known.map(|i| title(i.title.clone()).text_size(crate::theme::rem(12.5))),
                    )
                    .children(
                        known
                            .filter(|i| !i.labels.is_empty())
                            .map(|i| labels(&i.labels)),
                    )
                    .into_any_element()
            })
            .collect()
    }
}

/// A merged or closed pull request on one line: its icon, reference, title
/// and lines.
fn done_line(
    ix: usize,
    pr: &leon_domain::PullRequest,
    item: &GhRef,
    cx: &mut Context<Workspace>,
) -> AnyElement {
    opens(("info-pr", ix), item, cx)
        .py(crate::theme::rem(3.))
        .flex()
        .items_center()
        .gap_2()
        .text_size(crate::theme::rem(12.))
        .text_color(theme::fg_muted())
        .child(pr_icon(pr.state, 13.))
        .child(
            div()
                .flex_none()
                .font_family(theme::MONO)
                .child(item.short()),
        )
        .child(title(pr.title.clone()))
        .children(pr.lines.map(|(added, removed)| counts(added, removed)))
        .into_any_element()
}

/// A card that opens `item` in the details drawer.
fn card(id: (&'static str, usize), item: &GhRef, cx: &mut Context<Workspace>) -> Stateful<Div> {
    opens(id, item, cx)
        .py_2()
        .flex()
        .flex_col()
        .gap(crate::theme::rem(5.))
        .border_1()
        .border_color(theme::border())
        .bg(theme::window())
}

/// A row of the Info tab that opens `item` in the details drawer.
fn opens(id: (&'static str, usize), item: &GhRef, cx: &mut Context<Workspace>) -> Stateful<Div> {
    let item = item.clone();
    div()
        .id(id)
        // Its full height: the scrolling column never squeezes it.
        .flex_none()
        .mx_3()
        .px_3()
        .rounded_lg()
        .cursor_pointer()
        .hover(|s| s.bg(theme::surface_2()))
        .on_click(cx.listener(move |this, _, _, _| {
            this.core.send(Intent::ShowDetail(Some(item.clone())));
        }))
}

/// Its kind and reference: "Pull request acme/api#12".
fn header(glyph: gpui::Svg, what: &'static str, item: &GhRef) -> Div {
    div()
        .flex()
        .items_center()
        .gap_2()
        .child(glyph)
        .child(
            div()
                .flex_none()
                .text_size(crate::theme::rem(12.5))
                .child(what),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .font_family(theme::MONO)
                .text_size(crate::theme::rem(12.))
                .text_color(theme::fg_muted())
                .child(item.short()),
        )
}

/// A title on one line, cut short when the pane is narrow.
fn title(text: String) -> Div {
    div()
        .flex_1()
        .min_w_0()
        .overflow_hidden()
        .whitespace_nowrap()
        .text_ellipsis()
        .child(text)
}
