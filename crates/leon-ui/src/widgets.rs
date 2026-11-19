//! Small building blocks shared by the panes.

use gpui::{
    AnyElement, App, ClickEvent, Div, ElementId, IntoElement, ParentElement, SharedString,
    Stateful, Styled, Window, div, prelude::*,
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
        .text_size(crate::theme::rem(10.5))
        .text_color(theme::fg_faint())
        .child(text.to_uppercase())
}

/// The header of a group that folds (`key`, kept folded across restarts by
/// the core): a chevron, then what the caller adds. A click folds or
/// unfolds it.
pub fn fold_toggle(key: String, folded: bool) -> Stateful<Div> {
    let chevron = if folded {
        crate::icons::Icon::ChevronRight
    } else {
        crate::icons::Icon::ChevronDown
    };
    div()
        .id(SharedString::from(format!("fold-{key}")))
        .flex()
        .items_center()
        .gap_2()
        .cursor_pointer()
        .child(crate::icons::icon(chevron, theme::fg_muted(), 15.))
        .on_click(move |_, _, cx| {
            if let Some(core) = cx.try_global::<crate::markdown::Core>() {
                core.0
                    .send(leon_core::Intent::FoldGroup(key.clone(), !folded));
            }
        })
}

/// A page's column, centred in its pane: `base` wide plus half the room a
/// wider pane leaves, so a bigger window shows more of it; all of a pane
/// narrower than `base`.
pub fn centered(base: f32, content: impl IntoElement) -> Div {
    let column = div()
        .flex_basis(theme::rem(base))
        // Twice the share of each side's room.
        .flex_grow(2.)
        .flex_shrink(1.)
        .min_w_0()
        .flex()
        .flex_col()
        .child(content);
    div()
        .flex_none()
        .w_full()
        .flex()
        .child(div().flex_1())
        .child(column)
        .child(div().flex_1())
}

/// Faint text for empty states.
pub fn empty(text: impl Into<SharedString>) -> AnyElement {
    div()
        .px_3()
        .py_2()
        .text_size(crate::theme::rem(12.5))
        .text_color(theme::fg_faint())
        .child(text.into())
        .into_any_element()
}

/// Status shape (NAV-5): ring when running, diamond when waiting for the
/// user, filled red when failed, faint ring when idle. Colour and shape, no
/// emoji (QUAL-3).
pub fn status_dot(status: Status) -> Div {
    let base = div().size(crate::theme::rem(9.)).flex_none();
    match status {
        Status::Idle => base
            .rounded_full()
            .border_1()
            .border_color(theme::fg_faint()),
        Status::Running => base.rounded_full().border_2().border_color(theme::accent()),
        Status::NeedsInput => base.rounded(crate::theme::rem(2.)).bg(theme::warn()),
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
        .py(crate::theme::rem(3.))
        .rounded_md()
        .border_1()
        .text_size(crate::theme::rem(12.))
        .cursor_pointer()
        .child(text.into())
        .on_click(on_click)
}

/// What a click on a control does.
pub type OnClick = Box<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

/// A choice between a few options shown side by side; the chosen one is
/// filled. Each option: its label, whether it is chosen, what a click does.
pub fn segmented(id: &'static str, options: Vec<(SharedString, bool, OnClick)>) -> Div {
    div()
        .flex()
        .flex_none()
        .p(crate::theme::rem(2.))
        .gap(crate::theme::rem(2.))
        .rounded_md()
        .border_1()
        .border_color(theme::border_strong())
        .bg(theme::window())
        .children(
            options
                .into_iter()
                .enumerate()
                .map(|(ix, (text, on, click))| {
                    div()
                        .id((id, ix))
                        .px_2()
                        .py(crate::theme::rem(2.))
                        .rounded(crate::theme::rem(4.))
                        .text_size(crate::theme::rem(12.))
                        .cursor_pointer()
                        .when(on, |d| d.bg(theme::accent_soft()).text_color(theme::fg()))
                        .when(!on, |d| {
                            d.text_color(theme::fg_muted())
                                .hover(|s| s.text_color(theme::fg()))
                        })
                        .child(text)
                        .on_click(move |e, w, cx| click(e, w, cx))
                }),
        )
}

/// Height of every pane's top row, so the dividers under them line up.
pub const HEADER_HEIGHT: f32 = 48.;

/// A pane's top row: its title, a muted subtitle, then whatever the caller
/// adds on the right (after a spacer).
pub fn pane_header(title: impl Into<SharedString>, subtitle: Option<String>) -> Div {
    let title: SharedString = title.into();
    div()
        .flex_none()
        .h(crate::theme::rem(HEADER_HEIGHT))
        .px_5()
        .flex()
        .items_center()
        .gap_3()
        .border_b_1()
        .border_color(theme::border())
        .child(
            div()
                .flex_none()
                .text_size(crate::theme::rem(15.))
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .child(title),
        )
        .children(subtitle.map(|text| {
            div()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .text_size(crate::theme::rem(12.5))
                .text_color(theme::fg_faint())
                .child(text)
        }))
        .child(div().flex_1())
}
