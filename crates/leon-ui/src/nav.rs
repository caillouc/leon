//! The row at the bottom of the left pane: Connectors, Settings, Doctor,
//! side by side, each an icon over its name. The Doctor's icon carries a dot
//! in the colour of its worst finding.

use gpui::{AnyElement, Context, Rgba, SharedString, Window, div, prelude::*, px};
use leon_core::Intent;

use crate::doctor::worst;
use crate::icons::{Icon, icon};
use crate::theme;
use crate::workspace::{Screen, Workspace};

impl Workspace {
    pub(crate) fn render_bottom_nav(&self, cx: &mut Context<Self>) -> AnyElement {
        let doctor_dot = self.snapshot.doctor.as_ref().map(|(_, s)| worst(s));
        div()
            .flex_none()
            .flex()
            .gap_1()
            .p_1()
            .border_t_1()
            .border_color(theme::border())
            .child(self.nav_item(Screen::Tools, Icon::Connectors, "Connectors", None, cx))
            .child(self.nav_item(Screen::Settings, Icon::Settings, "Settings", None, cx))
            .child(self.nav_item(Screen::Doctor, Icon::Doctor, "Doctor", doctor_dot, cx))
            .into_any_element()
    }

    fn nav_item(
        &self,
        screen: Screen,
        glyph: Icon,
        name: &'static str,
        dot: Option<Rgba>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let selected = self.screen == screen;
        let color = if selected {
            theme::fg()
        } else {
            theme::fg_muted()
        };
        div()
            .id(SharedString::from(format!("nav-{name}")))
            .flex_1()
            .py(px(6.))
            .rounded_md()
            .flex()
            .flex_col()
            .items_center()
            .gap(px(3.))
            .cursor_pointer()
            .when(selected, |s| s.bg(theme::accent_soft()))
            .when(!selected, |s| s.hover(|s| s.bg(theme::surface_2())))
            .child(
                div()
                    .relative()
                    .child(icon(glyph, color, 17.))
                    .children(dot.map(|dot| {
                        div()
                            .absolute()
                            .top(px(-2.))
                            .right(px(-4.))
                            .size(px(8.))
                            .rounded_full()
                            .border_1()
                            .border_color(theme::panel())
                            .bg(dot)
                    })),
            )
            .child(div().text_size(px(11.)).text_color(color).child(name))
            .on_click(cx.listener(move |this, _, window, cx| this.open_screen(screen, window, cx)))
            .into_any_element()
    }

    /// Shows one of the bottom row's screens, refreshing what it shows.
    pub(crate) fn open_screen(
        &mut self,
        screen: Screen,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.close_detail();
        self.screen = screen;
        self.core.send(Intent::View(None));
        match screen {
            Screen::Tools => self.core.send(Intent::RefreshTools),
            Screen::Doctor => self.core.send(Intent::RunDoctor),
            Screen::Settings => self.load_settings_form(window, cx),
            Screen::Nothing
            | Screen::Conversation(_)
            | Screen::NewConversation
            | Screen::GitHub(_) => {}
        }
        cx.notify();
    }
}
