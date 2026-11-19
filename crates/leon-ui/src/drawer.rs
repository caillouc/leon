//! A large panel over the middle of the window (diffs, an issue, a pull
//! request), on a dimmed backdrop: nothing behind it scrolls or reacts, a
//! click on the backdrop or on Close closes it.

use std::rc::Rc;

use gpui::{AnyElement, App, ClickEvent, Div, IntoElement, Window, div, prelude::*, relative};

use crate::icons::{Icon, icon};
use crate::theme;

type OnClose = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

/// The drawer: `header` (its left part; Close is added), `body`, and an
/// optional `footer`. `id` tells it from another drawer open under it: with
/// the same ids, the one under takes the clicks meant for the one on top.
pub(crate) fn drawer(
    id: &'static str,
    header: Div,
    body: impl IntoElement,
    footer: Option<AnyElement>,
    on_close: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    let on_close: OnClose = Rc::new(on_close);
    let close = Rc::clone(&on_close);
    let header = div()
        .flex_none()
        .flex()
        .items_center()
        .gap_3()
        .px_4()
        .py_3()
        .border_b_1()
        .border_color(theme::border())
        .child(header.flex_1().min_w_0())
        .child(
            div()
                .id("drawer-close")
                .flex_none()
                .p(crate::theme::rem(5.))
                .rounded_md()
                .cursor_pointer()
                .hover(|s| s.bg(theme::surface_2()))
                .child(icon(Icon::Close, theme::fg_muted(), 16.))
                .on_click(move |e, w, cx| close(e, w, cx)),
        );
    let panel = div()
        .id("drawer")
        .w(relative(0.84))
        .max_w(crate::theme::rem(1400.))
        .h(relative(0.9))
        .flex()
        .flex_col()
        .rounded_xl()
        .border_1()
        .border_color(theme::border_strong())
        .bg(theme::window())
        .shadow_lg()
        .overflow_hidden()
        // A click inside the drawer does not reach the backdrop.
        .on_click(|_, _, cx| cx.stop_propagation())
        .child(header)
        .child(div().flex_1().min_h_0().flex().flex_col().child(body))
        .children(footer);
    div()
        .id(id)
        .absolute()
        .inset_0()
        .occlude()
        .flex()
        .items_center()
        .justify_center()
        .bg(theme::scrim())
        .on_click(move |e, w, cx| on_close(e, w, cx))
        .child(panel)
        .into_any_element()
}
