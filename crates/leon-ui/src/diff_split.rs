//! Side-by-side diffs (GIT-2): the divider between the old and the new
//! side, dragged left or right.

use std::rc::Rc;

use gpui::{
    AnyElement, Context, CursorStyle, IntoElement, MouseButton, MouseMoveEvent, div, prelude::*,
    relative,
};

use crate::diff_rows::Data;
use crate::theme;
use crate::workspace::Workspace;

/// The old side's share of the width, at least and at most.
const RANGE: (f32, f32) = (0.15, 0.85);
/// Width of the divider, wide enough to grab.
const DIVIDER: f32 = 7.;
/// The rows' side padding in the list.
const PAD: f32 = 16.;

/// One side-by-side row: the old line, the divider, the new line.
pub(crate) fn pair(data: &Rc<Data>, old: AnyElement, new: AnyElement) -> AnyElement {
    let ratio = data.split.unwrap_or(0.5);
    let view = data.view.clone();
    div()
        .flex()
        .child(
            div()
                .w(relative(ratio))
                .min_w_0()
                .overflow_hidden()
                .child(old),
        )
        .child(
            div()
                .id("diff-divider")
                .w(crate::theme::rem(DIVIDER))
                .flex_none()
                .flex()
                .justify_center()
                .cursor(CursorStyle::ResizeLeftRight)
                .child(div().w(crate::theme::rem(1.)).h_full().bg(theme::border()))
                .hover(|s| s.bg(theme::accent_soft()))
                .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                    cx.stop_propagation();
                    view.update(cx, |this, _| this.right.split_drag = true).ok();
                }),
        )
        .child(div().flex_1().min_w_0().overflow_hidden().child(new))
        .into_any_element()
}

impl Workspace {
    /// While the divider is dragged, the old side takes the share of the
    /// width left of the mouse.
    pub(crate) fn drag_split(&mut self, e: &MouseMoveEvent, cx: &mut Context<Self>) {
        if !self.right.split_drag {
            return;
        }
        if e.pressed_button != Some(MouseButton::Left) {
            self.right.split_drag = false;
            return;
        }
        let Some(area) = self.right.split_area.get() else {
            return;
        };
        let left = f32::from(area.origin.x) + PAD;
        let width = (f32::from(area.size.width) - 2. * PAD - DIVIDER).max(1.);
        let ratio = ((f32::from(e.position.x) - left) / width).clamp(RANGE.0, RANGE.1);
        if (ratio - self.right.split_ratio).abs() > 0.002 {
            self.right.split_ratio = ratio;
            cx.notify();
        }
    }
}
