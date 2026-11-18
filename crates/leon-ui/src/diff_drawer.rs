//! Every file's diff, large, in a drawer over the middle of the window
//! (GIT-2, GIT-3, GIT-5): opened at the file clicked in Changes or Branch,
//! scrolled through file after file, closed with Close or a click beside it.
//! Comments are written here, and the pending review is sent from here too.

use gpui::{AnyElement, Context, FontWeight, Window, div, prelude::*, px};

use crate::diff_rows::counts;
use crate::diff_view::DiffSource;
use crate::drawer::drawer;
use crate::theme;
use crate::widgets::{OnClick, segmented};
use crate::workspace::{Screen, Workspace};

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
        self.right.drawer = None;
        cx.notify();
    }

    /// The drawer, when it is open.
    pub(crate) fn render_drawer(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let Screen::Flow(id) = self.screen else {
            return None;
        };
        let open = self.right.drawer.as_mut()?;
        let source = open.source;
        let scroll_to = open.scroll_to.take();
        let summary = self.diff_files(id, source).ok().map(|diff| {
            let n = diff.files.len();
            let added: u32 = diff.files.iter().map(|f| f.added).sum();
            let removed: u32 = diff.files.iter().map(|f| f.removed).sum();
            div()
                .flex()
                .items_center()
                .gap_2()
                .text_size(px(12.5))
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
                    .text_size(px(14.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(match source {
                        DiffSource::Changes => "Uncommitted changes",
                        DiffSource::Branch => "Branch diff",
                    }),
            )
            .children(summary)
            .child(div().flex_1())
            .child(self.diff_layout_toggle(cx));
        let body = self.diff_rows(id, source, scroll_to.as_deref(), cx);
        let review = self.render_review_bar(id, window, cx);
        Some(drawer(
            header,
            body,
            review,
            cx.listener(|this, _, _, cx| this.close_drawer(cx)),
        ))
    }
}
