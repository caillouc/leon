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

/// Width of the drawer's list of files, at first, at least and at most.
pub(crate) const FILES: f32 = 280.;
const FILES_RANGE: (f32, f32) = (160., 640.);

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
        let diff = self.diff_files(id, source).ok();
        // Its files down the side, as in the Changes and Branch tabs: a
        // click goes to that file (GIT-2).
        let files = diff.as_ref().map(|diff| {
            div()
                .id("drawer-files")
                .w(crate::theme::rem(self.right.files_width))
                .flex_none()
                .h_full()
                .overflow_y_scroll()
                .border_r_1()
                .border_color(theme::border())
                .bg(theme::panel())
                .children(self.file_rows(id, source, &diff.files, cx))
        });
        // Its right edge drags it wider or narrower.
        let edge = files.is_some().then(|| {
            div()
                .id("drawer-files-edge")
                .w(crate::theme::rem(8.))
                .mx(crate::theme::rem(-4.))
                .h_full()
                .flex_none()
                .cursor(gpui::CursorStyle::ResizeLeftRight)
                .hover(|s| s.bg(theme::accent_soft()))
                .on_mouse_down(
                    gpui::MouseButton::Left,
                    cx.listener(|this, _, _, cx| {
                        this.right.files_drag = true;
                        cx.stop_propagation();
                    }),
                )
        });
        let summary = diff.map(|diff| {
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
        // A flow's own diff opens in the editor (GIT-7); a pull request's,
        // GitHub's, says it is read-only and why.
        let (open, read_only) = if source == DiffSource::PullRequest {
            let why = div()
                .text_size(crate::theme::rem(12.))
                .text_color(theme::fg_muted())
                .child(
                    "Read-only: the pull request as GitHub shows it. To comment for an \
                     agent, open the flow's Changes or Branch diff, where its files are.",
                );
            (None, Some(why))
        } else {
            (self.open_worktree(id, cx), None)
        };
        let title = div()
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
            .children(open)
            .child(self.diff_layout_toggle(cx));
        let header = div()
            .flex()
            .flex_col()
            .gap_1()
            .child(title)
            .children(read_only);
        let area = std::rc::Rc::clone(&self.right.files_area);
        let body = div()
            .flex_1()
            .min_h_0()
            .flex()
            .relative()
            .on_mouse_move(cx.listener(|this, e: &gpui::MouseMoveEvent, _, cx| {
                this.drag_files(e, cx);
            }))
            .child(crate::widgets::probe(area))
            .children(files)
            .children(edge)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(self.diff_rows(id, source, scroll_to.as_deref(), cx)),
            );
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

    /// While the file list's edge is dragged, the list takes the width left
    /// of the mouse; the drag ends when the button is up.
    fn drag_files(&mut self, e: &gpui::MouseMoveEvent, cx: &mut Context<Self>) {
        if !self.right.files_drag {
            return;
        }
        if e.pressed_button != Some(gpui::MouseButton::Left) {
            self.right.files_drag = false;
            return;
        }
        let Some(area) = self.right.files_area.get() else {
            return;
        };
        let zoom = self.snapshot.settings.zoom.factor();
        let width =
            (f32::from(e.position.x - area.origin.x) / zoom).clamp(FILES_RANGE.0, FILES_RANGE.1);
        if (width - self.right.files_width).abs() > 0.5 {
            self.right.files_width = width;
            cx.notify();
        }
    }
}
