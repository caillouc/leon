//! The left and right panes' widths, changed by dragging their inner edge
//! (NAV-1). Kept while the window is open.

use gpui::{
    AnyElement, Context, CursorStyle, MouseButton, MouseDownEvent, MouseMoveEvent, div, prelude::*,
    px,
};

use crate::theme;
use crate::workspace::Workspace;

const LEFT: (f32, f32) = (200., 480.);
const RIGHT: (f32, f32) = (320., 900.);
/// Dragged narrower than this, the right pane closes.
const RIGHT_CLOSES: f32 = 200.;
/// The main pane never gets narrower than this while dragging.
const MAIN_MIN: f32 = 420.;

/// Which pane's edge is dragged.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Side {
    Left,
    Right,
}

/// The panes' widths and the edge being dragged.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Panes {
    pub left: f32,
    pub right: f32,
    /// The right pane shows; closed, a button in the flow's header
    /// opens it again.
    pub right_open: bool,
    pub resizing: Option<Side>,
}

impl Default for Panes {
    fn default() -> Self {
        Self {
            left: 268.,
            right: 440.,
            right_open: true,
            resizing: None,
        }
    }
}

impl Panes {
    /// Moves the dragged edge to `x` in a window `window` wide, within each
    /// pane's limits and leaving the main pane room.
    pub(crate) fn drag_to(&mut self, x: f32, window: f32) {
        let fit = |wanted: f32, (min, max): (f32, f32), other: f32| {
            wanted.clamp(min, max.min(window - other - MAIN_MIN).max(min))
        };
        match self.resizing {
            Some(Side::Left) => self.left = fit(x, LEFT, self.right),
            Some(Side::Right) => {
                let wanted = window - x;
                self.right_open = wanted >= RIGHT_CLOSES;
                if self.right_open {
                    self.right = fit(wanted, RIGHT, self.left);
                }
            }
            None => {}
        }
    }
}

impl Workspace {
    /// The thin strip over a pane's inner edge that drags it.
    pub(crate) fn pane_handle(side: Side, cx: &mut Context<Self>) -> AnyElement {
        div()
            .id(match side {
                Side::Left => "resize-left",
                Side::Right => "resize-right",
            })
            .w(px(8.))
            .mx(px(-4.))
            .h_full()
            .flex_none()
            .cursor(CursorStyle::ResizeLeftRight)
            .hover(|s| s.bg(theme::accent_soft()))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _: &MouseDownEvent, _, cx| {
                    this.panes.resizing = Some(side);
                    cx.stop_propagation();
                }),
            )
            .into_any_element()
    }

    /// Follows the mouse while an edge is dragged; the drag ends when the
    /// button is up.
    pub(crate) fn resize_panes(
        &mut self,
        e: &MouseMoveEvent,
        window: &gpui::Window,
        cx: &mut Context<Self>,
    ) {
        if self.panes.resizing.is_none() {
            return;
        }
        if e.pressed_button == Some(MouseButton::Left) {
            let width = f32::from(window.viewport_size().width);
            self.panes.drag_to(f32::from(e.position.x), width);
        } else {
            self.panes.resizing = None;
        }
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::{Panes, Side};

    #[test]
    fn edges_stay_within_limits_and_leave_the_main_pane_room() {
        let cases = [
            // side, mouse x, window width, (left, right) after
            (Side::Left, 320., 1600., (320., 440.)),
            (Side::Left, 50., 1600., (200., 440.)),
            (Side::Left, 900., 1600., (480., 440.)),
            (Side::Left, 480., 1100., (240., 440.)),
            (Side::Right, 1200., 1600., (268., 400.)),
            (Side::Right, 1350., 1600., (268., 320.)),
            (Side::Right, 100., 1600., (268., 900.)),
            (Side::Right, 100., 1200., (268., 512.)),
        ];
        for (side, x, window, expected) in cases {
            let mut panes = Panes {
                resizing: Some(side),
                ..Panes::default()
            };
            panes.drag_to(x, window);
            assert_eq!((panes.left, panes.right), expected, "{side:?} {x} {window}");
        }
        // Dragged far enough, the right pane closes, keeping its width for
        // when it opens again; dragged back, it opens.
        let mut panes = Panes {
            resizing: Some(Side::Right),
            ..Panes::default()
        };
        panes.drag_to(1450., 1600.);
        assert_eq!((panes.right_open, panes.right), (false, 440.));
        panes.drag_to(1200., 1600.);
        assert_eq!((panes.right_open, panes.right), (true, 400.));
        let mut idle = Panes::default();
        idle.drag_to(300., 1600.);
        assert_eq!(idle, Panes::default());
    }
}
