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
    div()
        .id(SharedString::from(format!("fold-{key}")))
        .flex()
        .items_center()
        .gap_2()
        .cursor_pointer()
        .child(crate::icons::chevron(!folded, theme::fg_muted(), 15.))
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

/// `order` with `item` moved to place `to`: what a drag and drop of a box
/// onto the one at `to` does (the CLI order, the projects).
pub fn moved<T: Copy + PartialEq>(order: &[T], item: T, to: usize) -> Vec<T> {
    let mut next: Vec<T> = order.iter().copied().filter(|x| *x != item).collect();
    next.insert(to.min(next.len()), item);
    next
}

/// Text edited where it reads — a flow's name, a to-do's part: a quiet box
/// (`input` without its own look: no border, no focus ring) on a soft
/// background, its text where it was and at the body's size.
pub fn in_place<T: gpui_component::Sizable + Styled + IntoElement>(input: T) -> Div {
    // The box's own inset (an extra-small one's, 4 px), taken back.
    div()
        .mx(gpui::px(-4.))
        .rounded(crate::theme::rem(4.))
        .bg(theme::surface_2())
        .child(input.xsmall().text_size(crate::theme::rem(13.)))
}

/// Where an element is on screen, as [`probe`] last saw it.
pub type Area = std::rc::Rc<std::cell::Cell<Option<gpui::Bounds<gpui::Pixels>>>>;

/// An invisible element reporting its parent's place on screen each frame
/// (the parent `relative`): what a drag reads, e.g. the diff list's width
/// to turn the mouse's place into a share of it.
pub fn probe(area: Area) -> AnyElement {
    gpui::canvas(move |bounds, _, _| area.set(Some(bounds)), |_, (), _, _| {})
        .absolute()
        .size_full()
        .into_any_element()
}

/// Faint text for empty states.
pub fn empty(text: impl Into<SharedString>) -> AnyElement {
    note(text.into(), theme::fg_faint())
}

/// What went wrong, where an empty state would show: in red, so it is not
/// taken for "nothing here".
pub fn problem(text: impl Into<SharedString>) -> AnyElement {
    note(text.into(), theme::err())
}

fn note(text: SharedString, color: gpui::Rgba) -> AnyElement {
    div()
        .px_3()
        .py_2()
        .text_size(crate::theme::rem(12.5))
        .text_color(color)
        .child(text)
        .into_any_element()
}

/// A state's mark (design.md, Status): a small square, `size` wide, filled
/// with the state's `color` — a check's result, a board column, connected.
/// Round dots are for notifications only.
pub fn mark(color: gpui::Rgba, size: f32) -> Div {
    square(size).bg(color)
}

/// The empty square every status mark is drawn in.
fn square(size: f32) -> Div {
    div()
        .size(crate::theme::rem(size))
        .flex_none()
        .rounded(crate::theme::rem(2.))
}

/// A conversation's status mark (NAV-5), a small square like the working
/// loader: outlined in blue when running, filled amber when waiting for the
/// user, filled red when failed, faint outline when idle. Colour and shape,
/// no emoji (QUAL-3).
pub fn status_dot(status: Status) -> Div {
    match status {
        Status::Idle => square(9.).border_1().border_color(theme::fg_faint()),
        Status::Running => square(9.).border_2().border_color(theme::accent()),
        Status::NeedsInput => mark(theme::warn(), 9.),
        Status::Error => mark(theme::err(), 9.),
    }
}

/// A small icon button: icon 13 muted, then `label` (11.5 muted) when
/// given, its background on hover (copy a branch, open a worktree in the
/// editor). The click stays its own.
pub fn icon_button(
    id: impl Into<ElementId>,
    glyph: crate::icons::Icon,
    label: Option<SharedString>,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    div()
        .id(id)
        .flex_none()
        .flex()
        .items_center()
        .gap(crate::theme::rem(4.))
        .p(crate::theme::rem(3.))
        .rounded(crate::theme::rem(4.))
        .text_size(crate::theme::rem(11.5))
        .text_color(theme::fg_muted())
        .whitespace_nowrap()
        .cursor_pointer()
        .hover(|s| s.bg(theme::surface_2()))
        .child(crate::icons::icon(glyph, theme::fg_muted(), 13.))
        .children(label)
        .on_click(move |e, window, cx| {
            cx.stop_propagation();
            on_click(e, window, cx);
        })
}

/// A small text action ("+ Add project", "Open in Zed"): 11.5 muted, its
/// background on hover.
pub fn text_action(
    id: impl Into<ElementId>,
    text: impl Into<SharedString>,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    div()
        .id(id)
        .flex_none()
        .px(crate::theme::rem(6.))
        .py(crate::theme::rem(1.))
        .rounded_md()
        .text_size(crate::theme::rem(11.5))
        .text_color(theme::fg_muted())
        .whitespace_nowrap()
        .cursor_pointer()
        .hover(|s| s.bg(theme::surface_2()).text_color(theme::fg()))
        .child(text.into())
        .on_click(on_click)
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
        // The to-dos button's room, at the top right.
        .pr(crate::theme::rem(crate::todos_view::BUTTON_ROOM))
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

/// A pill (design.md): `text` in `color`, on that colour at 18 %, 10.5
/// semibold, `scale` times as big (the Flow tab's zoom).
pub(crate) fn pill(text: impl Into<SharedString>, color: gpui::Rgba, scale: f32) -> Div {
    div()
        .flex_none()
        .px(gpui::px(6. * scale))
        .rounded_full()
        .bg(gpui::Rgba { a: 0.18, ..color })
        .text_size(theme::rem(10.5 * scale))
        .font_weight(gpui::FontWeight::SEMIBOLD)
        .text_color(color)
        .child(text.into())
}

#[cfg(test)]
mod tests {
    use super::moved;

    #[test]
    fn a_dropped_box_takes_the_place_it_is_dropped_on() {
        let order = ['a', 'b', 'c'];
        let cases = [
            ('c', 0, vec!['c', 'a', 'b']),
            ('a', 2, vec!['b', 'c', 'a']),
            ('b', 1, vec!['a', 'b', 'c']),
            ('a', 9, vec!['b', 'c', 'a']),
        ];
        for (item, to, expected) in cases {
            assert_eq!(moved(&order, item, to), expected, "{item} to {to}");
        }
    }
}
