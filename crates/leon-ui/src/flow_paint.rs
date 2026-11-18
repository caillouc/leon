//! Drawing the flow diagram's arrows, and its legend (FLOW-4).

use gpui::{AnyElement, PathBuilder, Pixels, Point, Window, div, point, prelude::*, px};
use leon_domain::ArrowStyle;

use crate::flow::color;
use crate::flow_model::{EdgeKind, Pt, Step, bends};
use crate::theme;

/// Legend and gestures, bottom left.
pub(crate) fn legend() -> AnyElement {
    let key = |text: &'static str, kind: EdgeKind| {
        div()
            .flex()
            .items_center()
            .gap(px(5.))
            .child(div().w(px(18.)).h(px(2.)).bg(color(kind)))
            .child(text)
    };
    div()
        .absolute()
        .left_3()
        .top_3()
        .flex()
        .items_center()
        .gap_3()
        .px_2()
        .py(px(5.))
        .rounded_lg()
        .border_1()
        .border_color(theme::border())
        .bg(theme::surface())
        .text_size(px(11.5))
        .text_color(theme::fg_muted())
        .child(key("prompt", EdgeKind::Prompt))
        .child(key("report", EdgeKind::Report))
        .child(key("question", EdgeKind::Question))
        .into_any_element()
}

/// How to draw one arrow.
pub(crate) struct Arrow {
    pub kind: EdgeKind,
    /// Its messages are open in the inspector.
    pub wide: bool,
    pub scale: f32,
    pub style: ArrowStyle,
}

impl Arrow {
    /// The arrow along `points` (an orthogonal route, screen units from
    /// `origin`): straight segments with rounded bends, or one smooth curve;
    /// dashed for reports; with its head.
    pub(crate) fn paint(&self, window: &mut Window, origin: Point<Pixels>, points: &[Pt]) {
        let Self {
            kind,
            wide,
            scale,
            style,
        } = *self;
        let at = |p: Pt| point(origin.x + px(p.x), origin.y + px(p.y));
        let (Some(last), Some(before)) = (
            points.last(),
            points.len().checked_sub(2).and_then(|i| points.get(i)),
        ) else {
            return;
        };
        let (dx, dy) = (last.x - before.x, last.y - before.y);
        let len = (dx * dx + dy * dy).sqrt().max(1.);
        let (ux, uy) = (dx / len, dy / len);
        let head = 9. * scale.max(0.6);
        let tip_base = Pt {
            x: last.x - ux * head,
            y: last.y - uy * head,
        };
        let width = if wide { 2.8 } else { 1.6 };
        let mut line = PathBuilder::stroke(px(width));
        if kind == EdgeKind::Report {
            line = line.dash_array(&[px(5.), px(4.)]);
        }
        let body: Vec<Pt> = points
            .iter()
            .take(points.len() - 1)
            .copied()
            .chain(std::iter::once(tip_base))
            .collect();
        line.move_to(at(body[0]));
        match (style, body.as_slice()) {
            // Curved: the points are the curve's start, controls and end.
            (ArrowStyle::Curved, [_, a, b, end]) => {
                line.cubic_bezier_to(at(*end), at(*a), at(*b));
            }
            (ArrowStyle::Rounded | ArrowStyle::Curved, _) => {
                for step in bends(&body, 16. * scale) {
                    match step {
                        Step::Line(p) => line.line_to(at(p)),
                        Step::Curve { to, ctrl } => line.curve_to(at(to), at(ctrl)),
                    }
                }
            }
        }
        let c = color(kind);
        if let Ok(path) = line.build() {
            window.paint_path(path, c);
        }
        let (nx, ny) = (-uy * head / 2., ux * head / 2.);
        let mut arrow = PathBuilder::fill();
        arrow.move_to(at(*last));
        arrow.line_to(at(Pt {
            x: tip_base.x + nx,
            y: tip_base.y + ny,
        }));
        arrow.line_to(at(Pt {
            x: tip_base.x - nx,
            y: tip_base.y - ny,
        }));
        arrow.close();
        if let Ok(path) = arrow.build() {
            window.paint_path(path, c);
        }
    }
}
