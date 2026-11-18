//! What the composer and the new-conversation form share: long pasted text
//! kept as a folded chip (CONV-12) and the `/` list of skills and commands
//! (CONV-11).

use gpui::{
    AnyElement, App, ClipboardItem, Context, Entity, Focusable, IntoElement, SharedString, Window,
    div, prelude::*, px,
};
use gpui_component::input::{Paste, TextareaState};
use leon_core::ports::SlashCommand;
use leon_domain::ProjectId;

use crate::theme;
use crate::workspace::{Workspace, text_of};

/// A paste with at least this many lines, or this many characters, is folded.
const FOLD_LINES: usize = 10;
const FOLD_CHARS: usize = 800;

/// Most entries the `/` list shows.
const SLASH_LIMIT: usize = 8;

/// Whether a paste is long enough to fold.
fn folds(text: &str) -> bool {
    text.lines().count() >= FOLD_LINES || text.chars().count() >= FOLD_CHARS
}

/// The message to send: what was typed, then each folded paste.
pub(crate) fn message(typed: &str, pasted: &[String]) -> String {
    std::iter::once(typed.trim())
        .chain(pasted.iter().map(|p| p.trim_end()))
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
}

impl Workspace {
    /// Keeps a long paste out of the input, as a chip (CONV-12).
    pub(crate) fn on_paste(&mut self, cx: &mut Context<Self>) {
        let Some(text) = cx
            .read_from_clipboard()
            .and_then(|c: ClipboardItem| c.text())
        else {
            return;
        };
        if folds(&text) {
            self.pasted.push(text);
            cx.stop_propagation();
            cx.notify();
        }
    }

    /// Wraps an input so that long pastes become chips.
    pub(crate) fn paste_aware(input: gpui::Div, cx: &mut Context<Self>) -> gpui::Div {
        input.capture_action(cx.listener(|this, _: &Paste, _, cx| this.on_paste(cx)))
    }

    /// The chips of folded pastes, each removable.
    pub(crate) fn pasted_chips(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if self.pasted.is_empty() {
            return None;
        }
        let chips = self.pasted.iter().enumerate().map(|(ix, text)| {
            let lines = text.lines().count();
            div()
                .id(("pasted", ix))
                .flex()
                .items_center()
                .gap_1()
                .px_2()
                .py(px(2.))
                .rounded_md()
                .border_1()
                .border_color(theme::border_strong())
                .bg(theme::surface_2())
                .text_size(px(12.))
                .text_color(theme::fg_muted())
                .child(format!("Pasted text · {lines} lines"))
                .child(
                    div()
                        .id(("pasted-remove", ix))
                        .px_1()
                        .cursor_pointer()
                        .hover(|s| s.text_color(theme::fg()))
                        .child("×")
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if ix < this.pasted.len() {
                                this.pasted.remove(ix);
                            }
                            cx.notify();
                        })),
                )
        });
        Some(
            div()
                .flex()
                .flex_wrap()
                .gap_1()
                .children(chips)
                .into_any_element(),
        )
    }

    /// The `/` list while the input holds only `/` and a name prefix: click
    /// an entry to put `/name ` in the input.
    pub(crate) fn slash_list(
        &self,
        input: &Entity<TextareaState>,
        project: Option<ProjectId>,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let typed = text_of(input, cx);
        let commands = self.snapshot.commands.get(&project?)?;
        let matches = slash_matches(&typed, commands)?;
        let rows = matches.into_iter().enumerate().map(|(ix, c)| {
            let input = input.clone();
            let name = c.name.clone();
            // The first one is what Tab writes in.
            let first = ix == 0;
            div()
                .id(("slash", ix))
                .px_2()
                .py_1()
                .rounded_md()
                .flex()
                .gap_2()
                .cursor_pointer()
                .when(first, |d| d.bg(theme::surface_2()))
                .hover(|s| s.bg(theme::surface_2()))
                .child(
                    div()
                        .font_family(theme::MONO)
                        .text_size(px(12.))
                        .child(format!("/{}", c.name)),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .text_size(px(12.))
                        .text_color(theme::fg_muted())
                        .child(c.description.clone()),
                )
                .when(first, |d| {
                    d.child(
                        div()
                            .flex_none()
                            .text_size(px(11.))
                            .text_color(theme::fg_faint())
                            .child("⇥ Tab"),
                    )
                })
                .on_click(move |_, window, cx: &mut App| {
                    let text = SharedString::from(format!("/{name} "));
                    input.update(cx, |state, cx| state.set_value(text, window, cx));
                    input.focus_handle(cx).focus(window, cx);
                })
        });
        Some(
            div()
                .p_1()
                .rounded_lg()
                .border_1()
                .border_color(theme::border_strong())
                .bg(theme::surface())
                .children(rows)
                .into_any_element(),
        )
    }
}

/// The commands a message box starting with `/` matches, at most
/// `SLASH_LIMIT`; `None` when it does not start a command or nothing
/// matches.
pub(crate) fn slash_matches<'a>(
    typed: &str,
    commands: &'a [SlashCommand],
) -> Option<Vec<&'a SlashCommand>> {
    let prefix = typed.strip_prefix('/')?;
    if prefix.contains(char::is_whitespace) {
        return None;
    }
    let prefix = prefix.to_lowercase();
    let matches: Vec<_> = commands
        .iter()
        .filter(|c| c.name.to_lowercase().contains(&prefix))
        .take(SLASH_LIMIT)
        .collect();
    (!matches.is_empty()).then_some(matches)
}

/// What Tab writes in the box: the first matching command and a space.
pub(crate) fn slash_completion(typed: &str, commands: &[SlashCommand]) -> Option<String> {
    let first = slash_matches(typed, commands)?.into_iter().next()?;
    Some(format!("/{} ", first.name))
}

impl Workspace {
    /// Tab while the `/` list shows: writes its first command in `input`.
    /// Whether it did.
    pub(crate) fn complete_slash(
        &self,
        input: &Entity<TextareaState>,
        project: Option<ProjectId>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let typed = text_of(input, cx);
        let completion = project
            .and_then(|p| self.snapshot.commands.get(&p))
            .and_then(|commands| slash_completion(&typed, commands));
        let Some(text) = completion else {
            return false;
        };
        input.update(cx, |state, cx| state.set_value(text, window, cx));
        true
    }

    /// The agent's suggested next message (CONV-10) shows as the message
    /// box's placeholder while the box is empty; Tab writes it in.
    pub(crate) fn show_suggestion(
        &mut self,
        suggestion: Option<&str>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let placeholder = suggestion.map_or_else(
            || SharedString::from(PLACEHOLDER),
            |text| SharedString::from(format!("{text}    ⇥ Tab")),
        );
        // Set only when it changes: setting it redraws the box.
        if self.composer_placeholder != placeholder {
            self.composer_placeholder = placeholder.clone();
            self.composer.update(cx, |state, cx| {
                state.set_placeholder(placeholder, window, cx);
            });
        }
    }

    /// Tab in an empty message box: writes the suggestion in; otherwise Tab
    /// does what it does in a text box.
    pub(crate) fn accept_suggestion(
        &mut self,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !text_of(&self.composer, cx).is_empty() {
            return;
        }
        cx.stop_propagation();
        let text = text.to_owned();
        self.composer
            .update(cx, |state, cx| state.set_value(text, window, cx));
    }
}

/// The message box's placeholder without a suggestion.
pub(crate) const PLACEHOLDER: &str = "Message the agent";

#[cfg(test)]
mod tests {
    use leon_core::ports::SlashCommand;

    use super::{folds, message, slash_completion, slash_matches};

    #[test]
    fn long_pastes_fold_and_join_the_message() {
        assert!(!folds("one line"));
        assert!(folds(&"line\n".repeat(10)));
        assert!(folds(&"x".repeat(800)));
        let pasted = ["log line 1\nlog line 2\n".to_owned()];
        assert_eq!(
            message("Why does this fail?", &pasted),
            "Why does this fail?\n\nlog line 1\nlog line 2"
        );
        assert_eq!(message("  ", &pasted), "log line 1\nlog line 2");
        assert_eq!(message("hi", &[]), "hi");
    }

    #[test]
    fn tab_completes_the_first_matching_command() {
        let command = |name: &str| SlashCommand {
            name: name.into(),
            description: String::new(),
        };
        let commands = [command("review"), command("release"), command("deploy")];
        let names = |typed: &str| {
            slash_matches(typed, &commands)
                .map(|m| m.iter().map(|c| c.name.as_str()).collect::<Vec<_>>())
        };
        assert_eq!(names("/re"), Some(vec!["review", "release"]));
        assert_eq!(names("/"), Some(vec!["review", "release", "deploy"]));
        assert_eq!(names("/re now"), None);
        assert_eq!(names("re"), None);
        assert_eq!(names("/zzz"), None);
        assert_eq!(
            slash_completion("/rel", &commands).as_deref(),
            Some("/release ")
        );
        assert_eq!(slash_completion("/release now", &commands), None);
    }
}
