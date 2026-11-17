//! Small building blocks shared by the panes.

use gpui::{
    AnyElement, App, ClickEvent, Div, ElementId, IntoElement, ParentElement, SharedString,
    Stateful, Styled, Window, div, prelude::*, px,
};
use leon_domain::Status;

use crate::theme;

/// A section label: small, uppercase, faint.
pub fn label(text: impl Into<SharedString>) -> Div {
    let text: SharedString = text.into();
    div()
        .px_3()
        .pt_3()
        .pb_1()
        .text_size(px(10.5))
        .text_color(theme::fg_faint())
        .child(text.to_uppercase())
}

/// Faint text for empty states.
pub fn empty(text: impl Into<SharedString>) -> AnyElement {
    div()
        .px_3()
        .py_2()
        .text_size(px(12.5))
        .text_color(theme::fg_faint())
        .child(text.into())
        .into_any_element()
}

/// Status shape (NAV-5): ring when running, diamond when waiting for the
/// user, filled red when failed, faint ring when idle. Colour and shape, no
/// emoji (QUAL-3).
pub fn status_dot(status: Status) -> Div {
    let base = div().size(px(9.)).flex_none();
    match status {
        Status::Idle => base
            .rounded_full()
            .border_1()
            .border_color(theme::fg_faint()),
        Status::Running => base.rounded_full().border_2().border_color(theme::accent()),
        Status::NeedsInput => base.rounded(px(2.)).bg(theme::warn()),
        Status::Error => base.rounded_full().bg(theme::err()),
    }
}

/// A plain text button.
pub fn button(
    id: impl Into<ElementId>,
    text: impl Into<SharedString>,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    base_button(id, text, on_click)
        .border_color(theme::border_strong())
        .bg(theme::surface())
        .hover(|s| s.bg(theme::surface_2()))
}

/// A filled accent button.
pub fn primary(
    id: impl Into<ElementId>,
    text: impl Into<SharedString>,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    base_button(id, text, on_click)
        .bg(theme::accent())
        .border_color(theme::accent())
        .text_color(theme::on_accent())
        .hover(|s| s.opacity(0.9))
}

/// Shape and behaviour shared by buttons; GPUI allows one hover style per
/// element, so each variant sets its own.
fn base_button(
    id: impl Into<ElementId>,
    text: impl Into<SharedString>,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    div()
        .id(id.into())
        .px_2()
        .py(px(3.))
        .rounded_md()
        .border_1()
        .text_size(px(12.))
        .cursor_pointer()
        .child(text.into())
        .on_click(on_click)
}
