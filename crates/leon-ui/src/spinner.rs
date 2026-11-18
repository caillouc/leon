//! A turning loader for what an agent is working on (NAV-5). It turns one
//! step every 100 ms, and only while an agent works: the window then redraws
//! ten times a second, and not at all otherwise (GPUI's own animations
//! redraw every frame).

use std::f32::consts::TAU;
use std::time::Duration;

use gpui::{AnyElement, Context, IntoElement, Rgba, Svg, Transformation, radians};
use leon_domain::Status;

use crate::icons::{Icon, icon};
use crate::theme;
use crate::widgets::status_dot;
use crate::workspace::Workspace;

/// Steps in one turn.
const STEPS: u8 = 12;

/// The loader's angle at `step`.
fn angle(step: u8) -> f32 {
    f32::from(step % STEPS) * TAU / f32::from(STEPS)
}

impl Workspace {
    /// Starts the ticking when an agent starts working, stops it when none
    /// works (dropping the task ends it).
    pub(crate) fn keep_spinning(&mut self, cx: &mut Context<Self>) {
        let working = self
            .snapshot
            .sessions
            .values()
            .any(|s| s.status == Status::Running);
        if !working {
            self.spinner = None;
            return;
        }
        if self.spinner.is_some() {
            return;
        }
        self.spinner = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(100))
                    .await;
                let ticked = this.update(cx, |this, cx| {
                    this.spin = (this.spin + 1) % STEPS;
                    cx.notify();
                });
                if ticked.is_err() {
                    break;
                }
            }
        }));
    }

    /// The loader, turned to the current step.
    pub(crate) fn spinner(&self, color: Rgba, size: f32) -> Svg {
        icon(Icon::Working, color, size)
            .with_transformation(Transformation::rotate(radians(angle(self.spin))))
    }

    /// An agent session's status mark: the turning loader while it works.
    pub(crate) fn status_mark(&self, status: Status) -> AnyElement {
        match status {
            Status::Running => self.spinner(theme::accent(), 12.).into_any_element(),
            other => status_dot(other).into_any_element(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::f32::consts::TAU;

    use super::angle;

    #[test]
    fn twelve_steps_make_a_turn() {
        assert!(angle(0).abs() < f32::EPSILON);
        assert!((angle(3) - TAU / 4.).abs() < 1e-6);
        assert!(angle(12).abs() < f32::EPSILON);
    }
}
