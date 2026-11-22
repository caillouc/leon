//! A pull request and an issue as every list shows them (design.md, "PR
//! and issue row"): the GitHub lists (GH-2, GH-3), the Info tab (CTX-1) and
//! a flow's pull requests on hover (NAV-6). The icon in its state's colour,
//! the title, the number at its end; under it, its details. A click opens
//! it in Leon's details drawer.

use gpui::{AnyElement, Context, Div, FontWeight, IntoElement, SharedString, div, prelude::*};
use leon_core::Intent;
use leon_domain::{CheckState, FlowId, GhRef, Issue, PrState, PullRequest};

use crate::diff_rows::counts;
use crate::github::pr_icon;
use crate::icons::{Icon, icon};
use crate::pr_view::{check_color, labels};
use crate::sprints::board_chips;
use crate::theme::{self, rem};
use crate::workspace::Workspace;

/// A row: icon, title, its number at the end (no state word, no age: the
/// icon says the state, times since are noise); under it the details, unless
/// it is folded to that one line, then `muted` when its work is done.
/// `hover` floats over the row's right end while the mouse is on it, so it
/// takes no room otherwise.
struct Row {
    item: GhRef,
    hover: Option<AnyElement>,
    icon: AnyElement,
    title: String,
    meta: Option<Div>,
    muted: bool,
}

/// How much of a pull request a list shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Fold {
    /// Every detail: the GitHub lists.
    Never,
    /// One line once its work is done, merged or closed: Info.
    Done,
    /// One line each: a flow's pull requests on hover.
    Always,
}

impl Row {
    fn render(self) -> AnyElement {
        let Self {
            item,
            hover,
            icon,
            title,
            meta,
            muted,
        } = self;
        let folded = meta.is_none();
        let number = item.number;
        let group = SharedString::from(format!("gh-row-{}", item.text()));
        let heading = if folded {
            div()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .when(muted, |d| d.text_color(theme::fg_muted()))
        } else {
            div().font_weight(FontWeight::MEDIUM)
        };
        div()
            .id(group.clone())
            .group(group.clone())
            .relative()
            .px_3()
            .py(rem(if folded { 5. } else { 10. }))
            .rounded_lg()
            .flex()
            .items_start()
            .gap_3()
            .cursor_pointer()
            .hover(|s| s.bg(theme::surface()))
            .on_click(move |_, _, cx| {
                if let Some(core) = cx.try_global::<crate::markdown::Core>() {
                    core.0.send(Intent::ShowDetail(Some(item.clone())));
                }
            })
            .child(div().flex_none().pt(rem(2.)).child(icon))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(rem(5.))
                    .child(
                        div()
                            .flex()
                            .items_baseline()
                            .gap_2()
                            .child(heading.flex_1().min_w_0().child(title))
                            .child(
                                div()
                                    .flex_none()
                                    .text_size(rem(12.))
                                    .text_color(theme::fg_faint())
                                    .child(format!("#{number}")),
                            ),
                    )
                    .children(
                        meta.map(|meta| meta.text_size(rem(12.)).text_color(theme::fg_muted())),
                    ),
            )
            .children(hover.map(|action| {
                div()
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .right_3()
                    .flex()
                    .items_center()
                    .opacity(0.)
                    .group_hover(group, |s| s.opacity(1.))
                    .child(div().pl_4().bg(theme::surface()).child(action))
            }))
            .into_any_element()
    }
}

/// A pull request. Under its title: its repository, its branch into its
/// base, its lines, and while open its CI, conflicts and labels; `link`
/// ends that line. `fold` says when it shows as one line.
pub(crate) fn pr_row(pr: &PullRequest, fold: Fold, link: Option<AnyElement>) -> AnyElement {
    let open = matches!(pr.state, PrState::Open | PrState::Draft);
    let folded = match fold {
        Fold::Never => false,
        Fold::Done => !open,
        Fold::Always => true,
    };
    let meta = (!folded).then(|| {
        div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap_x_3()
            .gap_y_1()
            .child(repo_text(&pr.repo))
            .child(into_base(pr))
            .children(pr.lines.map(|(added, removed)| counts(added, removed)))
            .when(open, |d| d.children(ci(pr)))
            // Conflicts with its base, as GitHub says (GH-3).
            .when(open && pr.conflicts, |d| {
                d.child(chip(
                    Icon::Failed,
                    theme::err(),
                    format!("Conflicts with {}", pr.base),
                ))
            })
            .child(labels(&pr.labels))
            .children(link)
    });
    Row {
        item: GhRef::pull_request(&pr.repo, pr.number),
        hover: None,
        icon: pr_icon(pr.state, 16.).into_any_element(),
        title: pr.title.clone(),
        meta,
        muted: !open,
    }
    .render()
}

/// An issue Leon has not read yet: its repository and number.
pub(crate) fn issue_ref_row(item: &GhRef) -> AnyElement {
    Row {
        item: item.clone(),
        hover: None,
        icon: icon(Icon::Issue, theme::fg_faint(), 16.).into_any_element(),
        title: leon_domain::repo_name(&item.repo).to_owned(),
        meta: None,
        muted: false,
    }
    .render()
}

impl Workspace {
    /// An issue, its icon in its board column's colour. Under its title:
    /// its repository, board fields and labels. In the lists (`in_list`),
    /// a link to the flow started for it, else "Start conversation" on
    /// hover.
    pub(crate) fn issue_row(
        &self,
        issue: &Issue,
        in_list: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let item = GhRef::issue(&issue.repo, issue.number);
        let linked = self.flow_for_issue(&item).filter(|_| in_list);
        let meta = div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap_2()
            .child(repo_text(&issue.repo))
            .children(board_chips(issue))
            .child(labels(&issue.labels))
            .children(linked.map(|flow| flow_link(&item.text(), flow, cx)));
        Row {
            hover: (in_list && linked.is_none()).then(|| self.issue_action(issue, cx)),
            item,
            icon: icon(Icon::Issue, self.issue_color(issue.status.as_ref()), 16.)
                .into_any_element(),
            title: issue.title.clone(),
            meta: Some(meta),
            muted: false,
        }
        .render()
    }

    /// What Leon knows of issue `item`: an issue of mine or one linked to a
    /// flow.
    pub(crate) fn known_issue(&self, item: &GhRef) -> Option<&Issue> {
        self.snapshot
            .my_issues
            .iter()
            .find(|i| i.repo == item.repo && i.number == item.number)
            .or_else(|| self.snapshot.linked_issues.get(item))
    }
}

/// Its CI in one indicator, in its state's colour: failed, the failing
/// jobs by name, each opening its run; running, how many are done;
/// passing; skipped (CTX-1).
fn ci(pr: &PullRequest) -> Option<AnyElement> {
    let state = pr.checks_state()?;
    let color = check_color(state);
    let line = div()
        .flex()
        .items_center()
        .gap_2()
        .text_color(color)
        .child(icon(crate::pr_page::check_glyph(state), color, 12.));
    let all = pr.checks.len();
    let done = pr
        .checks
        .iter()
        .filter(|c| c.state != CheckState::Pending)
        .count();
    let key = format!("ci-{}-{}", pr.repo, pr.number);
    Some(
        match state {
            CheckState::Fail if all > 0 => line.children(
                pr.checks
                    .iter()
                    .filter(|c| c.state == CheckState::Fail)
                    .enumerate()
                    .map(|(ix, c)| job(format!("{key}-{ix}"), c)),
            ),
            CheckState::Pending if all > 0 => line.child(format!("{done}/{all} done")),
            // Without the jobs, GitHub's summary of them.
            CheckState::Fail => line.child("Checks failing"),
            CheckState::Pending => line.child("Checks running"),
            CheckState::Pass => line.child("Checks passing"),
            CheckState::Skipped => line.child("Checks skipped"),
        }
        .into_any_element(),
    )
}

/// A failed job's name, opening its run on GitHub when it has one.
fn job(id: String, check: &leon_domain::Check) -> AnyElement {
    let name = div().id(SharedString::from(id)).child(check.name.clone());
    match check.url.clone() {
        Some(url) => name
            .cursor_pointer()
            .hover(|s| s.text_color(theme::fg()))
            .on_click(move |_, _, cx| {
                // Not also a click on the row, which opens the details.
                cx.stop_propagation();
                cx.open_url(&url);
            })
            .into_any_element(),
        None => name.into_any_element(),
    }
}

/// An icon and a few words, in `color`.
fn chip(glyph: Icon, color: gpui::Rgba, text: String) -> Div {
    div()
        .flex()
        .items_center()
        .gap(rem(4.))
        .text_color(color)
        .child(icon(glyph, color, 12.))
        .child(text)
}

/// Which branch a pull request merges, into which: "fix-retry → dev", the
/// destination brighter, so two PRs of one change into two branches part.
fn into_base(pr: &PullRequest) -> Div {
    div()
        .flex()
        .items_center()
        .gap(rem(5.))
        .min_w_0()
        .font_family(theme::MONO)
        .text_size(rem(11.5))
        .child(icon(Icon::Branch, theme::fg_muted(), 12.))
        .child(
            div()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .child(pr.branch.clone()),
        )
        .child(div().flex_none().child("→"))
        .child(
            div()
                .flex_none()
                .text_color(theme::fg())
                .child(pr.base.clone()),
        )
}

fn repo_text(repo: &str) -> Div {
    div()
        .font_family(theme::MONO)
        .text_size(rem(11.5))
        .child(leon_domain::repo_name(repo).to_owned())
}

/// "Open flow" as a link: in a row's details line, or by a to-do group's
/// title; `key` tells it apart in the window.
pub(crate) fn flow_link(key: &str, flow: FlowId, cx: &mut Context<Workspace>) -> AnyElement {
    div()
        .id(SharedString::from(format!("open-flow-{key}")))
        .flex()
        .items_center()
        .gap(rem(4.))
        .text_color(theme::accent())
        .cursor_pointer()
        .hover(|s| s.opacity(0.8))
        .child(icon(Icon::Flow, theme::accent(), 12.))
        .child("Open flow")
        .on_click(cx.listener(move |this, _, _, cx| {
            cx.stop_propagation();
            this.select(flow, cx);
        }))
        .into_any_element()
}
