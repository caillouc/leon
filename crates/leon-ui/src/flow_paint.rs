//! Drawing the flow diagram's arrows, and its legend (FLOW-4).

use std::time::Duration;

use gpui::{
    Animation, AnimationExt as _, AnyElement, Bounds, ElementId, PathBuilder, Pixels, Point, Rgba,
    Window, canvas, div, point, prelude::*, px,
};

use crate::flow::color;
use crate::flow_model::{EdgeKind, Pt, Step, bends};
use crate::theme;

/// The legend, bottom left.
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
        .bottom_3()
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
        // Clicks here do not start dragging the diagram.
        .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .into_any_element()
}

/// How to draw one arrow.
pub(crate) struct Arrow {
    pub kind: EdgeKind,
    /// It goes both ways: a head at each end.
    pub both: bool,
    /// Its messages are open in the inspector.
    pub wide: bool,
    pub scale: f32,
}

impl Arrow {
    /// The arrow along `points` (an orthogonal route, screen units from
    /// `origin`): straight segments with rounded bends, or one smooth curve;
    /// dashed for reports; with its head, and one at its start too when it
    /// goes both ways.
    pub(crate) fn paint(&self, window: &mut Window, origin: Point<Pixels>, points: &[Pt]) {
        let Self {
            kind,
            both,
            wide,
            scale,
        } = *self;
        let at = |p: Pt| point(origin.x + px(p.x), origin.y + px(p.y));
        let n = points.len();
        if n < 2 {
            return;
        }
        let head = 9. * scale.max(0.6);
        let c = color(kind);
        let tip_base = arrowhead(window, &at, points[n - 1], points[n - 2], head, c);
        let start = if both {
            arrowhead(window, &at, points[0], points[1], head, c)
        } else {
            points[0]
        };
        let width = if wide { 2.8 } else { 1.6 };
        let mut line = PathBuilder::stroke(px(width));
        if kind == EdgeKind::Report {
            line = line.dash_array(&[px(5.), px(4.)]);
        }
        let body: Vec<Pt> = std::iter::once(start)
            .chain(points[1..n - 1].iter().copied())
            .chain(std::iter::once(tip_base))
            .collect();
        line.move_to(at(body[0]));
        for step in bends(&body, 16. * scale) {
            match step {
                Step::Line(p) => line.line_to(at(p)),
                Step::Curve { to, ctrl } => line.curve_to(at(to), at(ctrl)),
            }
        }
        if let Ok(path) = line.build() {
            window.paint_path(path, c);
        }
    }
}

/// Paints a head of `size` at `tip`, coming from `before`; returns where
/// the line meets its base.
fn arrowhead(
    window: &mut Window,
    at: &impl Fn(Pt) -> Point<Pixels>,
    tip: Pt,
    before: Pt,
    size: f32,
    c: Rgba,
) -> Pt {
    let (dx, dy) = (tip.x - before.x, tip.y - before.y);
    let len = (dx * dx + dy * dy).sqrt().max(1.);
    let (ux, uy) = (dx / len, dy / len);
    let base = Pt {
        x: tip.x - ux * size,
        y: tip.y - uy * size,
    };
    let (nx, ny) = (-uy * size / 2., ux * size / 2.);
    let mut arrow = PathBuilder::fill();
    arrow.move_to(at(tip));
    arrow.line_to(at(Pt {
        x: base.x + nx,
        y: base.y + ny,
    }));
    arrow.line_to(at(Pt {
        x: base.x - nx,
        y: base.y - ny,
    }));
    arrow.close();
    if let Ok(path) = arrow.build() {
        window.paint_path(path, c);
    }
    base
}

/// One lap of a working box's agent colour around its border.
const LAP: Duration = Duration::from_millis(2400);

/// A working box's agent colour going round its border (FLOW-4): a stretch
/// as long as the box is wide, starting where its band sits at the top,
/// `width` thick over the box's edge, whose corners have `radius`.
pub(crate) fn circling(
    id: impl Into<ElementId>,
    color: Rgba,
    radius: f32,
    width: f32,
) -> AnyElement {
    div()
        .absolute()
        .inset_0()
        .with_animation(id, Animation::new(LAP).repeat(), move |layer, delta| {
            layer.child(
                canvas(
                    |_, _, _| (),
                    move |bounds, (), window, _| {
                        lap(window, bounds, radius, width, delta, color);
                    },
                )
                .size_full(),
            )
        })
        .into_any_element()
}

/// Paints the stretch of the border `delta` of a lap along.
fn lap(
    window: &mut Window,
    bounds: Bounds<Pixels>,
    radius: f32,
    width: f32,
    delta: f32,
    color: Rgba,
) {
    let (w, h) = (f32::from(bounds.size.width), f32::from(bounds.size.height));
    let ring = border(w, h, radius, width / 2.);
    let total = length(&ring);
    if total <= 0. {
        return;
    }
    let from = delta * total;
    let stretch = stretch(&ring, from, from + (w - 2. * radius).max(w / 2.));
    let at = |p: Pt| point(bounds.origin.x + px(p.x), bounds.origin.y + px(p.y));
    let mut line = PathBuilder::stroke(px(width));
    let Some(first) = stretch.first() else { return };
    line.move_to(at(*first));
    for p in &stretch[1..] {
        line.line_to(at(*p));
    }
    if let Ok(path) = line.build() {
        window.paint_path(path, color);
    }
}

/// The rounded rectangle `w` by `h`, `inset` inside its edge, clockwise
/// from the left end of its top side, as points close enough to draw.
#[allow(clippy::cast_precision_loss, reason = "a few points per corner")]
fn border(w: f32, h: f32, radius: f32, inset: f32) -> Vec<Pt> {
    const STEPS: usize = 8;
    let r = (radius - inset).max(0.);
    let (left, top, right, bottom) = (inset + r, inset + r, w - inset - r, h - inset - r);
    // Each corner's middle and the angle its arc starts at.
    let corners = [
        (right, top, -std::f32::consts::FRAC_PI_2),
        (right, bottom, 0.),
        (left, bottom, std::f32::consts::FRAC_PI_2),
        (left, top, std::f32::consts::PI),
    ];
    let mut ring = vec![Pt { x: left, y: inset }];
    for (cx, cy, start) in corners {
        for k in 0..=STEPS {
            let a = start + std::f32::consts::FRAC_PI_2 * k as f32 / STEPS as f32;
            ring.push(Pt {
                x: cx + r * a.cos(),
                y: cy + r * a.sin(),
            });
        }
    }
    ring
}

fn length(points: &[Pt]) -> f32 {
    points
        .windows(2)
        .map(|p| crate::flow_model::layout::distance(p[0], p[1]))
        .sum()
}

/// The part of the closed line through `ring` from `from` to `to` along
/// it, going round again past its end.
fn stretch(ring: &[Pt], from: f32, to: f32) -> Vec<Pt> {
    let mut out = Vec::new();
    let mut walked = 0.;
    for pair in ring.windows(2).chain(ring.windows(2)) {
        let (a, b) = (pair[0], pair[1]);
        let step = crate::flow_model::layout::distance(a, b);
        let (start, end) = (walked, walked + step);
        walked = end;
        if end <= from || step <= 0. {
            continue;
        }
        let along = |d: f32| {
            let t = ((d - start) / step).clamp(0., 1.);
            Pt {
                x: a.x + (b.x - a.x) * t,
                y: a.y + (b.y - a.y) * t,
            }
        };
        if out.is_empty() {
            out.push(along(from));
        }
        if end >= to {
            out.push(along(to));
            break;
        }
        out.push(b);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{border, length, stretch};
    use crate::flow_model::Pt;

    /// The stretch going round a box follows its border, past the end of
    /// a lap back to its start.
    #[test]
    fn the_working_stretch_goes_round_the_border() {
        // Square corners: the lap is the rectangle's perimeter.
        let ring = border(100., 40., 0., 0.);
        assert!((length(&ring) - 280.).abs() < 0.01, "{ring:?}");
        let near = |p: Pt, x: f32, y: f32| (p.x - x).abs() < 0.01 && (p.y - y).abs() < 0.01;
        // Along the top, then down the right side.
        let first = stretch(&ring, 50., 120.);
        assert!(near(first[0], 50., 0.), "{first:?}");
        assert!(near(first[first.len() - 1], 100., 20.), "{first:?}");
        // Past the end of the lap: on along the top again.
        let round = stretch(&ring, 270., 300.);
        assert!(near(round[0], 0., 10.), "{round:?}");
        assert!(near(round[round.len() - 1], 20., 0.), "{round:?}");
        // Round corners make the lap shorter.
        assert!(length(&border(100., 40., 10., 0.)) < 280.);
    }
}
