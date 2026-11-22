//! A line dragged to another place in its list: a project in the left pane
//! (NAV-2), a flow's group on the To do page (TODO-3). Lifted, it follows
//! the mouse up and down in its list's column, tilting with its speed like
//! a card held where the hand took it (`/` going down, `\` going up), and
//! rocks back level, through the other way, when the mouse stops.
//!
//! GPUI turns no box holding text: the line is drawn in thin upright
//! strips, each moved up or down by its distance from the hand, so the
//! whole of it leans.

use std::rc::Rc;
use std::time::{Duration, Instant};

use gpui::{
    AnyElement, App, Context, Div, DragMoveEvent, IntoElement, Pixels, Point, Render, Rgba,
    SharedString, Window, div, prelude::*, px,
};
use leon_core::Intent;
use leon_domain::{FlowId, ProjectId};

use crate::theme;
use crate::workspace::Workspace;

/// A project being dragged to another place.
#[derive(Debug, Clone)]
pub(crate) struct DraggedProject(pub ProjectId);

/// A flow's group being dragged to another place on the To do page.
#[derive(Debug, Clone)]
pub(crate) struct DraggedTodoFlow(pub FlowId);

/// How far the line tilts, in degrees per pixel a second of mouse speed.
const TILT_PER_SPEED: f32 = 0.012;
/// The most it tilts, in degrees.
const MAX_TILT: f32 = 4.;
/// How fast its tilt catches up with the mouse's speed, in seconds.
const EASE: f32 = 0.05;
/// How long the mouse keeps still before the line rocks back, in seconds:
/// between two moves it keeps its tilt.
const STILL: f32 = 0.05;
/// How fast its rocking dies out, in seconds.
const SETTLE: f32 = 0.12;
/// How fast it rocks back and forth, in radians a second.
const SWING: f32 = 30.;
/// How long after the mouse stops it may still rock.
const SWING_FOR: Duration = Duration::from_millis(600);
/// The width of the strips the line is drawn in, in pixels: thin enough
/// that their steps do not show.
const STRIP: f32 = 2.;
/// Room kept around the line for its shadow, in pixels.
const SHADOW: f32 = 24.;
/// The line's height, in rems.
const HEIGHT: f32 = 30.;

/// What a dragged line starts with.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Lead {
    /// A project's folder.
    Folder,
    /// A flow's group's mark, in its colour.
    Mark(Rgba),
}

/// The dragged line, drawn by GPUI under the mouse.
pub(crate) struct DragLine {
    lead: Lead,
    name: SharedString,
    count: usize,
    width: Pixels,
    /// Where the hand holds it, from its left edge: what it turns about.
    grip: Pixels,
    tilt: Tilt,
}

impl DragLine {
    /// The line of `name`, led by `lead`, with `count` things in it,
    /// `width` wide, held `grip` from its left edge.
    pub(crate) fn new(
        (lead, name, count): (Lead, SharedString, usize),
        width: Pixels,
        grip: Pixels,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Self {
        // While it rocks back the mouse is still: the window redraws only
        // on input, so it is asked to, until the drag ends.
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(16))
                    .await;
                let Ok(rocking) = this.update(cx, |line, _| line.tilt.rocking(Instant::now()))
                else {
                    break;
                };
                if rocking {
                    cx.update(App::refresh_windows);
                }
            }
        })
        .detach();
        Self {
            lead,
            name,
            count,
            width,
            grip,
            tilt: Tilt::new(window.mouse_position(), Instant::now()),
        }
    }

    /// The line itself, level: lead, name, count, in a lifted box.
    fn line(&self, width: Pixels) -> Div {
        let lead: AnyElement = match self.lead {
            Lead::Folder => crate::icons::folder(false, theme::fg_muted(), 15.).into_any_element(),
            Lead::Mark(color) => crate::widgets::mark(color, 10.).into_any_element(),
        };
        div()
            .w(width)
            .h(theme::rem(HEIGHT))
            .px_2()
            .flex()
            .items_center()
            .gap_2()
            .rounded_md()
            .border_1()
            .border_color(theme::accent())
            .bg(theme::surface())
            .shadow_lg()
            .child(lead)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .text_size(theme::rem(13.))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(theme::fg())
                    .child(self.name.clone()),
            )
            .child(
                div()
                    .text_size(theme::rem(12.))
                    .text_color(theme::fg_faint())
                    .child(self.count.to_string()),
            )
    }
}

/// Listens for a `T` dragged over `item`, one of a list in `order` (as it
/// was before the drag): the dragged one would go just before `item` while
/// the mouse is on its upper half, just after on its lower half; over its
/// own place it stays where it would go, where it was at first (so it
/// folds to its line as the drag starts, before it reaches the others).
/// `slot` keeps what is dragged and that place, so the list shows the
/// order a drop would give meanwhile.
/// Halves, not where `item` shows: as the list makes room, `item` moves
/// away from the mouse, never back under it, so the order holds still.
pub(crate) fn over<T: 'static, Id: Copy + PartialEq + 'static>(
    item: Id,
    order: Rc<[Id]>,
    id: fn(&T) -> Id,
    slot: fn(&mut Workspace) -> &mut Option<(Id, usize)>,
    cx: &mut Context<Workspace>,
) -> impl Fn(&DragMoveEvent<T>, &mut Window, &mut App) + 'static {
    cx.listener(move |this, e: &DragMoveEvent<T>, _, cx| {
        let mouse = e.event.position;
        if !e.bounds.contains(&mouse) {
            return;
        }
        let dragged = id(e.drag(cx));
        let slot = slot(this);
        let below = mouse.y > e.bounds.center().y;
        let to = if dragged == item {
            slot.filter(|(d, _)| *d == dragged)
                .map(|(_, to)| to)
                .or_else(|| order.iter().position(|i| *i == dragged))
        } else {
            place(&order, dragged, item, below)
        };
        let Some(to) = to else {
            return;
        };
        if *slot != Some((dragged, to)) {
            *slot = Some((dragged, to));
            cx.notify();
        }
    })
}

/// The place `dragged` takes in `order` let go over `item`, on its lower
/// half (`below`) or its upper one: its index among the others with it put
/// back. `None` over an item not among the others.
fn place<Id: Copy + PartialEq>(order: &[Id], dragged: Id, item: Id, below: bool) -> Option<usize> {
    let at = order
        .iter()
        .filter(|i| **i != dragged)
        .position(|i| *i == item)?;
    Some(at + usize::from(below))
}

/// Listens for a `T` let go over a list shown in `order`: the place in
/// `slot` becomes the dragged one's, sent with `intent` if it moved.
pub(crate) fn dropped<T: 'static, Id: Copy + PartialEq + 'static>(
    order: Vec<Id>,
    slot: fn(&mut Workspace) -> &mut Option<(Id, usize)>,
    intent: fn(Vec<Id>) -> Intent,
    cx: &mut Context<Workspace>,
) -> impl Fn(&T, &mut Window, &mut App) + 'static {
    cx.listener(move |this, _: &T, _, _| {
        if let Some((dragged, to)) = slot(this).take() {
            let next = crate::widgets::moved(&order, dragged, to);
            if next != order {
                this.core.send(intent(next));
            }
        }
    })
}

/// `order` as its list shows it while `drag` (what is dragged and the
/// place it would take) lasts.
pub(crate) fn shown<Id: Copy + PartialEq>(order: &[Id], drag: Option<(Id, usize)>) -> Vec<Id> {
    drag.map_or_else(
        || order.to_vec(),
        |(dragged, to)| crate::widgets::moved(order, dragged, to),
    )
}

/// How the line tilts as the mouse goes up and down.
#[derive(Debug, Clone, Copy)]
struct Tilt {
    /// Where the mouse was across when the drag started.
    start_x: f32,
    /// Where the mouse was last seen up and down, and when.
    last_y: f32,
    last_at: Instant,
    /// How far it tilted when the mouse last moved, in degrees.
    lean: f32,
}

impl Tilt {
    fn new(mouse: Point<Pixels>, now: Instant) -> Self {
        Self {
            start_x: f32::from(mouse.x),
            last_y: f32::from(mouse.y),
            last_at: now,
            lean: 0.,
        }
    }

    /// The line's tilt in degrees, clockwise (`\`) when positive, with the
    /// mouse at `mouse` at `now`.
    fn angle(&mut self, mouse: Point<Pixels>, now: Instant) -> f32 {
        let since = now.duration_since(self.last_at).as_secs_f32();
        let y = f32::from(mouse.y);
        if (y - self.last_y).abs() > f32::EPSILON {
            let speed = (y - self.last_y) / since.max(0.008);
            // From where it is now, eased: one jerk does not throw it.
            let from = settle(self.lean, since);
            self.lean = from + (lean(speed) - from) * (1. - (-since / EASE).exp());
            self.last_y = y;
            self.last_at = now;
        }
        settle(self.lean, now.duration_since(self.last_at).as_secs_f32())
    }

    fn rocking(&self, now: Instant) -> bool {
        self.lean.abs() > 0.05 && now.duration_since(self.last_at) < SWING_FOR
    }
}

/// The tilt for a speed up or down, in pixels a second: the far end lags,
/// so going down it tilts `/` (counter-clockwise), at most [`MAX_TILT`].
fn lean(speed: f32) -> f32 {
    -(speed * TILT_PER_SPEED).clamp(-MAX_TILT, MAX_TILT)
}

/// Where a line that tilted `lean` is `still` seconds after the mouse last
/// moved: tilted still for [`STILL`], then rocking back.
fn settle(lean: f32, still: f32) -> f32 {
    swing(lean, (still - STILL).max(0.))
}

/// Where a line that tilted `lean` is `t` seconds after it started rocking
/// back: through level, less each time.
fn swing(lean: f32, t: f32) -> f32 {
    lean * (-t / SETTLE).exp() * (SWING * t).cos()
}

impl Render for DragLine {
    fn render(&mut self, window: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let mouse = window.mouse_position();
        let slope = self.tilt.angle(mouse, Instant::now()).to_radians().tan();
        let rem = window.rem_size();
        let (width, height) = (self.width, theme::rem(HEIGHT).to_pixels(rem));
        // How far down a point `x` along the line goes as it tilts.
        let grip = self.grip;
        let rise = move |x: Pixels| (x - grip) * slope;
        let reach = rise(px(0.)).abs().max(rise(width).abs());
        let room = px(SHADOW) + reach;
        // Strips across the line and the room around it, in its frame.
        let mut strips = Vec::new();
        let mut x = -px(SHADOW);
        while x < width + px(SHADOW) {
            strips.push(
                div()
                    .absolute()
                    .left(x + px(SHADOW))
                    .top_0()
                    .w(px(STRIP))
                    .h_full()
                    .overflow_hidden()
                    .child(
                        self.line(width)
                            .absolute()
                            .left(-x)
                            .top(room + rise(x + px(STRIP / 2.))),
                    ),
            );
            x += px(STRIP);
        }
        // GPUI draws it under the mouse: back to its column, less the room.
        div()
            .relative()
            .left(px(self.tilt.start_x) - mouse.x - px(SHADOW))
            .top(-room)
            .w(width + px(2. * SHADOW))
            .h(height + room * 2.)
            .cursor(gpui::CursorStyle::ClosedHand)
            .children(strips)
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use gpui::{point, px};

    use super::{MAX_TILT, Tilt, lean, place, swing};

    #[test]
    fn it_goes_before_or_after_the_one_under_the_mouse() {
        let order = [1, 2, 3, 4];
        // (dragged, over, on its lower half, place).
        let cases = [
            (1, 3, false, Some(1)),
            (1, 3, true, Some(2)),
            (4, 1, false, Some(0)),
            (4, 1, true, Some(1)),
            (2, 4, true, Some(3)),
            // Not among the others.
            (2, 2, true, None),
            (2, 9, true, None),
        ];
        for (dragged, over, below, expected) in cases {
            assert_eq!(
                place(&order, dragged, over, below),
                expected,
                "{dragged} on {over}"
            );
        }
    }

    #[test]
    fn it_tilts_with_the_motion_the_far_end_lagging() {
        let cases = [
            (0., 0.),
            // Going down: `/`.
            (250., -3.),
            // Going up: `\`.
            (-250., 3.),
            (5_000., -MAX_TILT),
            (-5_000., MAX_TILT),
        ];
        for (speed, expected) in cases {
            assert!((lean(speed) - expected).abs() < 1e-4, "{speed}");
        }
    }

    #[test]
    fn a_steady_drag_down_tilts_it_then_it_rocks_back_level() {
        let start = Instant::now();
        let mut tilt = Tilt::new(point(px(100.), px(100.)), start);
        // Down at 300 pixels a second, an event every 16 ms, for 0.3 s.
        let mut angle = 0.;
        for step in 1..=19_u16 {
            let at = start + Duration::from_millis(u64::from(step) * 16);
            angle = tilt.angle(point(px(100.), px(100. + f32::from(step) * 4.8)), at);
        }
        assert!((-MAX_TILT..-2.5).contains(&angle), "{angle}");
        // The mouse stops: it rocks back through level, the other way…
        let stopped = start + Duration::from_millis(19 * 16);
        let mut still = |ms| {
            tilt.angle(
                point(px(100.), px(191.2)),
                stopped + Duration::from_millis(ms),
            )
        };
        let other_way = (1..=20_u64).map(|i| still(i * 10)).fold(f32::MIN, f32::max);
        assert!(other_way > 0.3, "{other_way}");
        // …then rests.
        assert!(still(700).abs() < 0.05);
    }

    #[test]
    fn it_rocks_back_through_level_and_settles() {
        assert!((swing(4., 0.) - 4.).abs() < 1e-4);
        // Half a swing later, a little the other way: the bounce.
        let back = swing(4., std::f32::consts::PI / super::SWING);
        assert!(back < -0.5 && back > -4., "{back}");
        assert!(swing(4., 0.6).abs() < 0.1);
    }
}
