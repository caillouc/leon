//! The loader for what an agent is working on (NAV-5): a square that drops,
//! squashes and turns a quarter turn over its shadow, every 0.5 s — after
//! alexruix's loader on Uiverse.io. GPUI draws the window each frame while
//! one shows, so only working agents get it.

use std::time::Duration;

use gpui::{
    Animation, AnimationExt as _, AnyElement, Bounds, ElementId, IntoElement, PathBuilder, Pixels,
    Rgba, Window, canvas, div, point, prelude::*, px, size,
};
use leon_domain::Status;

use crate::theme;
use crate::widgets::status_dot;

/// One jump.
const PERIOD: Duration = Duration::from_millis(500);
/// The square's side, in the loader's own size.
const SIDE: f32 = 0.55;

/// The loader; `id` is unique among the loaders on screen.
pub(crate) fn spinner(id: impl Into<ElementId>, color: Rgba, size: f32) -> AnyElement {
    div()
        .size(theme::rem(size))
        .flex_none()
        .with_animation(id, Animation::new(PERIOD).repeat(), move |d, t| {
            d.child(
                canvas(
                    |_, _, _| (),
                    move |bounds, (), window, _| paint(bounds, &frame(t), color, window),
                )
                .size_full(),
            )
        })
        .into_any_element()
}

/// A conversation's status mark, the loader while it works; always as
/// wide, so names line up. `id` is unique among the marks on screen.
pub(crate) fn status_mark(id: impl Into<ElementId>, status: Status) -> AnyElement {
    let mark = match status {
        Status::Running => spinner(id, theme::accent(), 12.),
        other => status_dot(other).into_any_element(),
    };
    div()
        .size(crate::theme::rem(12.))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .child(mark)
        .into_any_element()
}

/// The square at one moment of its jump, in square sides.
#[derive(Debug, PartialEq)]
struct Frame {
    /// How far down it is.
    drop: f32,
    /// Its turn, in degrees.
    turn: f32,
    /// Its height, squashed when it lands.
    squash: f32,
    /// Its bottom-right corner's radius; the others stay small.
    corner: f32,
    /// Its shadow's width.
    shadow: f32,
}

/// The original's keyframes (48 px square), at `t` of a jump (0 to 1),
/// linear between them.
fn frame(t: f32) -> Frame {
    let px48 = |v: f32| v / 48.;
    Frame {
        drop: px48(between(
            &[(0., 0.), (0.25, 9.), (0.5, 18.), (0.75, 9.), (1., 0.)],
            t,
        )),
        turn: 90. * t,
        squash: between(&[(0., 1.), (0.25, 1.), (0.5, 0.9), (0.75, 1.), (1., 1.)], t),
        corner: px48(between(&[(0., 4.), (0.15, 3.), (0.5, 40.), (1., 4.)], t)),
        shadow: between(&[(0., 1.), (0.5, 1.2), (1., 1.)], t),
    }
}

/// The value at `t` between `keys` (time, value), sorted by time.
fn between(keys: &[(f32, f32)], t: f32) -> f32 {
    let i = keys
        .partition_point(|(at, _)| *at <= t)
        .clamp(1, keys.len().saturating_sub(1).max(1));
    let ((t0, v0), (t1, v1)) = (keys[i - 1], keys[i]);
    if t1 <= t0 {
        return v1;
    }
    v0 + (v1 - v0) * ((t - t0) / (t1 - t0)).clamp(0., 1.)
}

/// The square's outline, side 1 round the origin, turned and squashed:
/// each corner a curve from one side to the next.
fn outline(f: &Frame) -> Vec<[(f32, f32); 3]> {
    let small = 4. / 48.;
    let corners = [
        ((-0.5, -0.5), small),
        ((0.5, -0.5), small),
        ((0.5, 0.5), f.corner),
        ((-0.5, 0.5), small),
    ];
    let (sin, cos) = f.turn.to_radians().sin_cos();
    let place = |(x, y): (f32, f32)| (x * cos - y * sin, (x * sin + y * cos) * f.squash);
    (0..4)
        .map(|i| {
            let (p, r) = corners[i];
            let (prev, _) = corners[(i + 3) % 4];
            let (next, _) = corners[(i + 1) % 4];
            // Sides are 1 long: a step of `r` towards a neighbour.
            let toward = |q: (f32, f32)| (p.0 + (q.0 - p.0) * r, p.1 + (q.1 - p.1) * r);
            [place(toward(prev)), place(p), place(toward(next))]
        })
        .collect()
}

/// Paints the shadow, then the square over it, in `bounds`.
fn paint(bounds: Bounds<Pixels>, frame: &Frame, color: Rgba, window: &mut Window) {
    let room = f32::from(bounds.size.width.min(bounds.size.height));
    let side = room * SIDE;
    // Top of the square at rest, so that its lowest point stays inside.
    let top = f32::from(bounds.origin.y) + (room - 1.51 * side) / 2.;
    let mid = f32::from(bounds.center().x);
    let (width, height) = (side * frame.shadow, side * 5. / 48.);
    window.paint_quad(
        gpui::fill(
            Bounds::new(
                point(px(mid - width / 2.), px(top + side * 60. / 48.)),
                size(px(width), px(height)),
            ),
            Rgba { a: 0.3, ..color },
        )
        .corner_radii(px(height / 2.)),
    );
    let centre = top + side / 2. + frame.drop * side;
    let at = |(x, y): (f32, f32)| point(px(mid + x * side), px(centre + y * side));
    let mut path = PathBuilder::fill();
    for (i, [from, corner, to]) in outline(frame).into_iter().enumerate() {
        if i == 0 {
            path.move_to(at(from));
        } else {
            path.line_to(at(from));
        }
        path.curve_to(at(to), at(corner));
    }
    path.close();
    if let Ok(path) = path.build() {
        window.paint_path(path, color);
    }
}

#[cfg(test)]
mod tests {
    use super::{Frame, frame, outline};

    fn close(a: &Frame, b: &Frame) -> bool {
        let near = |x: f32, y: f32| (x - y).abs() < 1e-4;
        near(a.drop, b.drop)
            && near(a.turn, b.turn)
            && near(a.squash, b.squash)
            && near(a.corner, b.corner)
            && near(a.shadow, b.shadow)
    }

    #[test]
    fn the_jump_follows_the_keyframes() {
        let at = |drop: f32, turn, squash, corner: f32, shadow| Frame {
            drop: drop / 48.,
            turn,
            squash,
            corner: corner / 48.,
            shadow,
        };
        let cases = [
            (0., at(0., 0., 1., 4., 1.)),
            (0.15, at(5.4, 13.5, 1., 3., 1.06)),
            (0.25, at(9., 22.5, 1., 13.571_43, 1.1)),
            (0.5, at(18., 45., 0.9, 40., 1.2)),
            (0.75, at(9., 67.5, 1., 22., 1.1)),
            (1., at(0., 90., 1., 4., 1.)),
        ];
        for (t, expected) in cases {
            let got = frame(t);
            assert!(close(&got, &expected), "at {t}: {got:?}, not {expected:?}");
        }
    }

    #[test]
    fn landing_squashes_the_square() {
        // Turned 45°, its lowest corner is half a diagonal down, squashed.
        let lowest = |t| {
            outline(&frame(t))
                .iter()
                .flatten()
                .map(|p| p.1)
                .fold(f32::MIN, f32::max)
        };
        assert!((lowest(0.) - 0.5).abs() < 1e-4);
        assert!(lowest(0.5) < 0.5_f32.hypot(0.5) * 0.9 + 1e-4);
        assert!(lowest(0.5) > 0.5);
    }
}
