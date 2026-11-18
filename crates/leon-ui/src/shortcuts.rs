//! The menu bar and keyboard shortcuts (QUAL-*): the usual macOS menus (on
//! Linux, the same shortcuts), so Quit, Hide, Close, Copy, Paste… work, and
//! a standard Edit menu, where macOS adds Start Dictation and Emoji.
//!
//! Shortcuts of Leon's own: ⌘N new conversation, ⌘, Settings, ⌘L the message
//! box, ⌘K search, ⌃Tab / ⌃⇧Tab next / previous conversation, Esc closes
//! what is open on top (a menu, a picker, a drawer).

use gpui::{App, Context, Div, KeyBinding, KeyDownEvent, Menu, MenuItem, OsAction, actions};
use gpui_component::input::{Copy, Cut, Paste, Redo, SelectAll, Undo};

use crate::workspace::{Screen, Workspace};

actions!(
    leon,
    [
        Quit,
        Hide,
        HideOthers,
        ShowAll,
        Minimize,
        Zoom,
        CloseWindow,
        NewConversation,
        AddProject,
        OpenSettings,
        FocusMessage,
        FocusSearch,
        NextConversation,
        PreviousConversation,
    ]
);

/// Binds the shortcuts and sets the menu bar; app-wide actions are handled
/// here, the others by the workspace.
pub(crate) fn install(cx: &mut App) {
    cx.bind_keys(bindings());
    cx.on_action(|_: &Quit, cx| cx.quit());
    cx.on_action(|_: &Hide, cx| cx.hide());
    cx.on_action(|_: &HideOthers, cx| cx.hide_other_apps());
    cx.on_action(|_: &ShowAll, cx| cx.unhide_other_apps());
    cx.set_menus([
        Menu::new("Leon").items([
            MenuItem::action("Settings…", OpenSettings),
            MenuItem::separator(),
            MenuItem::action("Hide Leon", Hide),
            MenuItem::action("Hide Others", HideOthers),
            MenuItem::action("Show All", ShowAll),
            MenuItem::separator(),
            MenuItem::action("Quit Leon", Quit),
        ]),
        Menu::new("File").items([
            MenuItem::action("New Conversation", NewConversation),
            MenuItem::action("Add Project…", AddProject),
            MenuItem::separator(),
            MenuItem::action("Close Window", CloseWindow),
        ]),
        Menu::new("Edit").items([
            MenuItem::os_action("Undo", Undo, OsAction::Undo),
            MenuItem::os_action("Redo", Redo, OsAction::Redo),
            MenuItem::separator(),
            MenuItem::os_action("Cut", Cut, OsAction::Cut),
            MenuItem::os_action("Copy", Copy, OsAction::Copy),
            MenuItem::os_action("Paste", Paste, OsAction::Paste),
            MenuItem::os_action("Select All", SelectAll, OsAction::SelectAll),
        ]),
        Menu::new("Go").items([
            MenuItem::action("Message Box", FocusMessage),
            MenuItem::action("Search Conversations", FocusSearch),
            MenuItem::separator(),
            MenuItem::action("Next Conversation", NextConversation),
            MenuItem::action("Previous Conversation", PreviousConversation),
        ]),
        Menu::new("Window").items([
            MenuItem::action("Minimize", Minimize),
            MenuItem::action("Zoom", Zoom),
        ]),
    ]);
}

/// `key` with the platform's main modifier: ⌘ on macOS, Ctrl elsewhere.
/// (`KeyBinding::new` does not accept GPUI's `secondary-`.)
macro_rules! primary {
    ($key:literal) => {
        if cfg!(target_os = "macos") {
            concat!("cmd-", $key)
        } else {
            concat!("ctrl-", $key)
        }
    };
}

/// The shortcuts. `KeyBinding::new` panics on a keystroke it cannot parse,
/// so the test below builds them all.
fn bindings() -> Vec<KeyBinding> {
    vec![
        KeyBinding::new(primary!("q"), Quit, None),
        KeyBinding::new("cmd-h", Hide, None),
        KeyBinding::new("alt-cmd-h", HideOthers, None),
        KeyBinding::new("cmd-m", Minimize, None),
        KeyBinding::new(primary!("w"), CloseWindow, None),
        KeyBinding::new(primary!("n"), NewConversation, None),
        KeyBinding::new(primary!("shift-o"), AddProject, None),
        KeyBinding::new(primary!(","), OpenSettings, None),
        KeyBinding::new(primary!("l"), FocusMessage, None),
        KeyBinding::new(primary!("k"), FocusSearch, None),
        KeyBinding::new("ctrl-tab", NextConversation, None),
        KeyBinding::new("ctrl-shift-tab", PreviousConversation, None),
    ]
}

impl Workspace {
    /// The workspace's handlers, put on its root element.
    pub(crate) fn shortcut_handlers(&self, root: Div, cx: &mut Context<Self>) -> Div {
        use gpui::InteractiveElement as _;
        root.track_focus(&self.focus)
            .on_action(cx.listener(|_, _: &Minimize, window, _| window.minimize_window()))
            .on_action(cx.listener(|_, _: &Zoom, window, _| window.zoom_window()))
            .on_action(cx.listener(|_, _: &CloseWindow, window, _| window.remove_window()))
            .on_action(cx.listener(|this, _: &NewConversation, window, cx| {
                let project = this.form.project;
                this.open_new_conversation(project, None, window, cx);
            }))
            .on_action(cx.listener(|this, _: &AddProject, _, cx| this.add_project(cx)))
            .on_action(cx.listener(|this, _: &OpenSettings, window, cx| {
                this.open_screen(Screen::Settings, window, cx);
            }))
            .on_action(cx.listener(|this, _: &FocusMessage, window, cx| {
                let input = match this.screen {
                    Screen::NewConversation => this.form_text.clone(),
                    _ => this.composer.clone(),
                };
                gpui::Focusable::focus_handle(input.read(cx), cx).focus(window, cx);
            }))
            .on_action(cx.listener(|this, _: &FocusSearch, window, cx| {
                gpui::Focusable::focus_handle(this.search.read(cx), cx).focus(window, cx);
            }))
            .on_action(
                cx.listener(|this, _: &NextConversation, _, cx| this.step_conversation(1, cx)),
            )
            .on_action(cx.listener(|this, _: &PreviousConversation, _, cx| {
                this.step_conversation(-1, cx);
            }))
            .capture_key_down(cx.listener(|this, e: &KeyDownEvent, _, cx| {
                if e.keystroke.key == "escape" && this.close_top(cx) {
                    cx.stop_propagation();
                }
            }))
    }

    /// Closes what is open on top, the most recent kind first; `false` when
    /// nothing was.
    fn close_top(&mut self, cx: &mut Context<Self>) -> bool {
        let closed = if self.open_picker.take().is_some()
            || self.row_menu.take().is_some()
            || self.right.drawer.take().is_some()
        {
            true
        } else if self.snapshot.detail.is_some() {
            self.core.send(leon_core::Intent::ShowDetail(None));
            true
        } else {
            false
        };
        if closed {
            cx.notify();
        }
        closed
    }

    /// Opens the conversation `step` places after (or before) the open one,
    /// in the left pane's order (pinned first, then by project).
    fn step_conversation(&mut self, step: isize, cx: &mut Context<Self>) {
        let order: Vec<_> = self.left_order();
        if order.is_empty() {
            return;
        }
        let at = match self.screen {
            Screen::Conversation(id) => order.iter().position(|c| *c == id),
            _ => None,
        };
        let len = isize::try_from(order.len()).unwrap_or(isize::MAX);
        let next = at.map_or(0, |i| {
            (isize::try_from(i).unwrap_or(0) + step).rem_euclid(len)
        });
        if let Some(id) = usize::try_from(next).ok().and_then(|i| order.get(i)) {
            self.select(*id, cx);
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn every_shortcut_parses() {
        assert_eq!(super::bindings().len(), 12);
    }
}
