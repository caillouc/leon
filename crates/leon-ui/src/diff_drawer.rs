//! One file's diff, large, in a drawer over the middle of the window
//! (GIT-1, GIT-3): opened from a file in Changes or Branch, closed with ×
//! or a click beside it. Comments work as in the right pane.

use gpui::{AnyElement, Context, FontWeight, IntoElement, div, prelude::*, px, relative};

use crate::diff_view::{DiffSource, counts, path_text, status_letter};
use crate::theme;
use crate::widgets::button;
use crate::workspace::{Screen, Workspace};

impl Workspace {
    pub(crate) fn open_drawer(&mut self, source: DiffSource, path: String, cx: &mut Context<Self>) {
        if let Screen::Conversation(id) = self.screen {
            // A fresh list: the file opens at its top.
            self.right.lists.remove(&(id, source, true));
        }
        self.right.drawer = Some((source, path));
        cx.notify();
    }

    fn close_drawer(&mut self, cx: &mut Context<Self>) {
        self.right.drawer = None;
        cx.notify();
    }

    /// The drawer, when a file is open in it.
    pub(crate) fn render_drawer(&mut self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let Screen::Conversation(id) = self.screen else {
            return None;
        };
        let (source, path) = self.right.drawer.clone()?;
        let file = self.snapshot.git.get(&id).and_then(|git| {
            let files = match source {
                DiffSource::Changes => &git.changes,
                DiffSource::Branch => &git.branch_diff,
            };
            files.iter().find(|f| f.path == path).cloned()
        });
        let body = self.render_diff(id, source, Some(&path), cx);
        let header = div()
            .flex_none()
            .flex()
            .items_center()
            .gap_2()
            .px_4()
            .py_3()
            .border_b_1()
            .border_color(theme::border())
            .text_size(px(13.))
            .children(file.as_ref().map(|f| status_letter(f.status)))
            .child(path_text(&path).font_weight(FontWeight::MEDIUM))
            .children(file.as_ref().map(|f| counts(f.added, f.removed)))
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(theme::fg_faint())
                    .child(match source {
                        DiffSource::Changes => "Uncommitted",
                        DiffSource::Branch => "Branch",
                    }),
            )
            .child(button(
                "drawer-close",
                "Close",
                cx.listener(|this, _, _, cx| this.close_drawer(cx)),
            ));
        let panel = div()
            .id("drawer")
            .w(relative(0.84))
            .max_w(px(1400.))
            .h(relative(0.9))
            .flex()
            .flex_col()
            .rounded_xl()
            .border_1()
            .border_color(theme::border_strong())
            .bg(theme::window())
            .shadow_lg()
            .overflow_hidden()
            // A click inside the drawer does not reach the scrim.
            .on_click(|_, _, cx| cx.stop_propagation())
            .child(header)
            .child(div().flex_1().min_h_0().flex().flex_col().child(body));
        Some(
            div()
                .id("drawer-scrim")
                .absolute()
                .inset_0()
                .flex()
                .items_center()
                .justify_center()
                .bg(theme::scrim())
                .on_click(cx.listener(|this, _, _, cx| this.close_drawer(cx)))
                .child(panel)
                .into_any_element(),
        )
    }
}
