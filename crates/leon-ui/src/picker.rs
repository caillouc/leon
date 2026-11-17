//! A small dropdown: a button showing the current value and, when open, the
//! list of choices. Only one picker is open at a time (`Workspace::open_picker`).

use std::rc::Rc;

use gpui::{
    AnyElement, Context, IntoElement, SharedString, Window, anchored, deferred, div, prelude::*, px,
};

use crate::theme;
use crate::workspace::Workspace;

/// One choice: the value given back, and the text shown.
pub(crate) type Choice = (String, String);

type OnPick = dyn Fn(&mut Workspace, String, &mut Window, &mut Context<Workspace>);

impl Workspace {
    /// Renders a picker. `id` must be unique in the window.
    pub(crate) fn picker(
        &self,
        id: &str,
        current: &str,
        choices: Vec<Choice>,
        cx: &mut Context<Self>,
        on_pick: impl Fn(&mut Self, String, &mut Window, &mut Context<Self>) + 'static,
    ) -> AnyElement {
        let id: SharedString = id.to_owned().into();
        let open = self.open_picker.as_ref() == Some(&id);
        let toggle_id = id.clone();
        let trigger = div()
            .id(SharedString::from(format!("{id}-trigger")))
            .px_2()
            .py(px(2.))
            .rounded_md()
            .text_size(px(12.))
            .text_color(theme::fg_muted())
            .cursor_pointer()
            .hover(|s| s.bg(theme::surface_2()).text_color(theme::fg()))
            .when(open, |s| s.bg(theme::surface_2()))
            .child(format!("{current} ▾"))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.open_picker = if this.open_picker.as_ref() == Some(&toggle_id) {
                    None
                } else {
                    Some(toggle_id.clone())
                };
                cx.notify();
            }));
        let mut container = div().relative().child(trigger);
        if open {
            let on_pick: Rc<OnPick> = Rc::new(on_pick);
            let items = choices.into_iter().enumerate().map(|(ix, (value, text))| {
                let on_pick = Rc::clone(&on_pick);
                let selected = text == current;
                div()
                    .id(ix)
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .text_size(px(12.5))
                    .cursor_pointer()
                    .when(selected, |s| s.bg(theme::accent_soft()))
                    .hover(|s| s.bg(theme::surface_2()))
                    .child(text)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.open_picker = None;
                        on_pick(this, value.clone(), window, cx);
                        cx.notify();
                    }))
            });
            let list = div()
                .id(SharedString::from(format!("{id}-list")))
                .min_w(px(160.))
                .max_h(px(320.))
                .overflow_y_scroll()
                .p_1()
                .rounded_lg()
                .border_1()
                .border_color(theme::border_strong())
                .bg(theme::surface())
                .shadow_lg()
                .children(items)
                .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                    this.open_picker = None;
                    cx.notify();
                }));
            container = container.child(
                deferred(anchored().snap_to_window_with_margin(px(8.)).child(list))
                    .with_priority(1),
            );
        }
        container.into_any_element()
    }
}
