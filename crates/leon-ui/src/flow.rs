//! The flow tab (FLOW-4): who started which agent session.

use gpui::{AnyElement, Context, IntoElement, Window, div, prelude::*};
use leon_domain::ConversationId;

use crate::widgets::empty;
use crate::workspace::Workspace;

impl Workspace {
    pub(crate) fn render_flow(
        _id: ConversationId,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .flex_1()
            .child(empty(
                "The flow diagram arrives with agents starting other agents (M4).",
            ))
            .into_any_element()
    }
}
