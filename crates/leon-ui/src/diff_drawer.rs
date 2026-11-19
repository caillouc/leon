//! Every file's diff, large, in a drawer over the middle of the window
//! (GIT-2, GIT-3, GIT-5): opened at the file clicked in Changes or Branch,
//! scrolled through file after file, closed with Close or a click beside it.
//! Comments are written here, and the pending review is sent from here too.

use gpui::{AnyElement, Context, FontWeight, Window, div, prelude::*};

use crate::diff_rows::counts;
use crate::diff_view::DiffSource;
use crate::drawer::drawer;
use crate::theme;
use crate::widgets::{OnClick, segmented};
use crate::workspace::{Screen, Workspace};

/// Where a pull request's diff keeps its files' open state when no flow is
/// on screen: no flow has id 0 (SQLite ids start at 1).
const NO_FLOW: leon_domain::FlowId = leon_domain::FlowId(0);

/// The open drawer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Drawer {
    pub source: DiffSource,
    /// The file to scroll to on the next render.
    pub scroll_to: Option<String>,
}

impl Workspace {
    /// Opens the drawer at `path`, unfolding it if `diff.fold` folded it.
    pub(crate) fn open_drawer(&mut self, source: DiffSource, path: String, cx: &mut Context<Self>) {
        if let Screen::Flow(id) = self.screen {
            self.right.open.insert((id, source, path.clone()), true);
        }
        self.right.drawer = Some(Drawer {
            source,
            scroll_to: Some(path),
        });
        cx.notify();
    }

    /// Unified or side by side, for the drawer's diffs.
    fn diff_layout_toggle(&self, cx: &mut Context<Self>) -> AnyElement {
        let split = self.right.split;
        segmented(
            "diff-layout",
            [(false, "Unified"), (true, "Side by side")]
                .into_iter()
                .map(|(value, text)| {
                    let view = cx.entity().downgrade();
                    let click: OnClick = Box::new(move |_, _, cx| {
                        view.update(cx, |this, cx| {
                            this.right.split = value;
                            cx.notify();
                        })
                        .ok();
                    });
                    (text.into(), split == value, click)
                })
                .collect(),
        )
        .into_any_element()
    }

    pub(crate) fn close_drawer(&mut self, cx: &mut Context<Self>) {
        if self
            .right
            .drawer
            .as_ref()
            .is_some_and(|d| d.source == DiffSource::PullRequest)
        {
            self.core.send(leon_core::Intent::ShowPrDiff(None));
        }
        self.right.drawer = None;
        cx.notify();
    }

    /// A pull request's changes as GitHub shows them, in the drawer, over
    /// its view (GH-3).
    pub(crate) fn open_pr_diff(&mut self, pr: leon_domain::GhRef, cx: &mut Context<Self>) {
        self.core.send(leon_core::Intent::ShowPrDiff(Some(pr)));
        self.right.drawer = Some(Drawer {
            source: DiffSource::PullRequest,
            scroll_to: None,
        });
        cx.notify();
    }

    /// The drawer, when it is open.
    pub(crate) fn render_drawer(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let open = self.right.drawer.as_mut()?;
        let source = open.source;
        // A pull request's diff opens over any screen; its files' open state
        // is kept under `NO_FLOW`.
        let id = match self.screen {
            Screen::Flow(id) => id,
            _ if source == DiffSource::PullRequest => NO_FLOW,
            _ => return None,
        };
        let open = self.right.drawer.as_mut()?;
        let scroll_to = open.scroll_to.take();
        let summary = self.diff_files(id, source).ok().map(|diff| {
            let n = diff.files.len();
            let added: u32 = diff.files.iter().map(|f| f.added).sum();
            let removed: u32 = diff.files.iter().map(|f| f.removed).sum();
            div()
                .flex()
                .items_center()
                .gap_2()
                .text_size(crate::theme::rem(12.5))
                .text_color(theme::fg_muted())
                .child(format!("{n} file{}", if n == 1 { "" } else { "s" }))
                .child(counts(added, removed))
                .child(diff.against)
        });
        let header = div()
            .flex()
            .items_center()
            .gap_3()
            .child(
                div()
                    .text_size(crate::theme::rem(14.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(match source {
                        DiffSource::Changes => "Uncommitted changes",
                        DiffSource::Branch => "Branch diff",
                        DiffSource::PullRequest => "Files changed",
                    }),
            )
            .children(summary)
            .child(div().flex_1())
            .child(self.diff_layout_toggle(cx));
        let body = self.diff_rows(id, source, scroll_to.as_deref(), cx);
        // Comments go to an agent from a flow's own diffs only.
        let review = (source != DiffSource::PullRequest)
            .then(|| self.render_review_bar(id, window, cx))
            .flatten();
        Some(drawer(
            "diff-drawer",
            header,
            body,
            review,
            cx.listener(|this, _, _, cx| this.close_drawer(cx)),
        ))
    }
}
