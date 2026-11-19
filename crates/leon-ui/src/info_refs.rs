//! The Info tab's pull requests and issues (CTX-1, CTX-2), each a card that
//! opens in Leon's details drawer first, GitHub from there. A pull request
//! stands out: its state's colour along its left side, its state, its title
//! in bold, its lines and its checks that did not pass. An issue stays plain.

use gpui::{AnyElement, Context, Div, FontWeight, IntoElement, Stateful, div, prelude::*, px};
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
                let (color, state) = pr_state(pr.state);
                let failing = pr.checks.iter().filter(|c| c.state != CheckState::Pass);
                card(("info-pr", ix), &item, cx)
                    .relative()
                    .overflow_hidden()
                    .pl(px(15.))
                    .child(
                        div()
                            .absolute()
                            .left_0()
                            .top_0()
                            .bottom_0()
                            .w(px(3.))
                            .bg(color),
                    )
                    .child(
                        header(pr_icon(pr.state, 14.), "Pull request", &item).child(
                            div()
                                .flex_none()
                                .px(px(6.))
                                .rounded(px(4.))
                                .border_1()
                                .border_color(color)
                                .text_size(px(11.))
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
                                    .text_size(px(13.))
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
                                    .text_size(px(12.5))
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
                        icon(Icon::Issue, theme::fg_muted(), 14.),
                        "Issue",
                        item,
                    ))
                    .children(known.map(|i| title(i.title.clone()).text_size(px(12.5))))
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

/// A card that opens `item` in the details drawer.
fn card(id: (&'static str, usize), item: &GhRef, cx: &mut Context<Workspace>) -> Stateful<Div> {
    let item = item.clone();
    div()
        .id(id)
        // Its full height: the scrolling column never squeezes it.
        .flex_none()
        .mx_3()
        .px_3()
        .py_2()
        .flex()
        .flex_col()
        .gap(px(5.))
        .rounded_lg()
        .border_1()
        .border_color(theme::border())
        .bg(theme::window())
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
        .child(div().flex_none().text_size(px(12.5)).child(what))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .font_family(theme::MONO)
                .text_size(px(12.))
                .text_color(theme::fg_muted())
                .child(item.text()),
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
