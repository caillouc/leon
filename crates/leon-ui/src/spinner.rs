//! A turning loader for what an agent is working on (NAV-5): one smooth
//! turn every 0.9 s. GPUI draws the window each frame while one shows, so
//! only working agents get it.

use std::time::Duration;

use gpui::{
    Animation, AnimationExt as _, AnyElement, ElementId, IntoElement, Rgba, Transformation, div,
    percentage, prelude::*,
};
use leon_domain::Status;

use crate::icons::{Icon, icon};
use crate::theme;
use crate::widgets::status_dot;

/// The loader; `id` is unique among the loaders on screen.
pub(crate) fn spinner(id: impl Into<ElementId>, color: Rgba, size: f32) -> AnyElement {
    icon(Icon::Working, color, size)
        .with_animation(
            id,
            Animation::new(Duration::from_millis(900)).repeat(),
            |svg, delta| svg.with_transformation(Transformation::rotate(percentage(delta))),
        )
        .into_any_element()
}

/// An conversation's status mark, the loader while it works; always as
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
