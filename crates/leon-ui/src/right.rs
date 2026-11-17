//! Right pane (CTX-1, GIT-*, GH-*): changes, branch diff, pull request.

use gpui::{AnyElement, Context, IntoElement, div, prelude::*, px};

use crate::theme;
use crate::widgets::empty;
use crate::workspace::Workspace;

const WIDTH: f32 = 420.;

impl Workspace {
    pub(crate) fn render_right(_cx: &mut Context<Self>) -> AnyElement {
        div()
            .w(px(WIDTH))
            .h_full()
            .flex_none()
            .bg(theme::panel())
            .border_l_1()
            .border_color(theme::border())
            .child(empty("Changes, branch diff and pull request show here."))
            .into_any_element()
    }
}
