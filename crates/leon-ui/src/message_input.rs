//! What the composer and the new-flow form share: long pasted text
//! kept as a folded chip (CONV-12) and the `/` list of skills and commands
//! (CONV-11).

use gpui::{
    AnyElement, App, ClipboardItem, ContentMask, Context, Entity, Focusable, IntoElement,
    SharedString, TextAlign, TextRun, Window, canvas, deferred, div, point, prelude::*, px,
};
use gpui_component::input::{InlineToken, InputContent, Paste, TextareaState};
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

/// The message to send: the text, each folded paste put back where its box
/// is.
pub(crate) fn message(content: &InputContent, pasted: &[String]) -> String {
    let text = content.text();
    let mut out = String::new();
    let mut at = 0;
    for span in content.tokens() {
        let range = span.range();
        out.push_str(&text[at..range.start]);
        let paste = span
            .token()
            .id()
            .strip_prefix(PASTE_ID)
            .and_then(|ix| ix.parse::<usize>().ok())
            .and_then(|ix| pasted.get(ix));
        out.push_str(paste.map_or(&text[range.clone()], |p| p.trim_end()));
        at = range.end;
    }
    out.push_str(&text[at..]);
    out.trim().to_owned()
}

/// Ids of folded pastes' boxes: this, then the paste's index.
const PASTE_ID: &str = "paste-";

/// The box a long paste folds into, in the text where it was pasted
/// (CONV-12): its text in the input, its label on screen.
fn paste_token(ix: usize, text: &str) -> InlineToken {
    InlineToken::new(
        format!("{PASTE_ID}{ix}"),
        format!("[Pasted text #{}]", ix + 1),
    )
    .with_label(format!("Pasted text · {} lines", text.lines().count()))
}

impl Workspace {
    /// Folds a long paste into a box where the caret is (CONV-12); the
    /// text goes back in its place when the message is sent.
    fn on_paste(
        &mut self,
        input: &Entity<TextareaState>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(text) = cx
            .read_from_clipboard()
            .and_then(|c: ClipboardItem| c.text())
            .filter(|text| folds(text))
        else {
            return;
        };
        let token = paste_token(self.pasted.len(), &text);
        let folded = input.update(cx, |state, cx| state.replace_with_token(token, window, cx));
        match folded {
            Ok(()) => {
                self.pasted.push(text);
                cx.stop_propagation();
                cx.notify();
            }
            // Pasted as plain text instead.
            Err(error) => tracing::debug!(?error, "could not fold a paste"),
        }
    }

    /// Wraps `input`'s box so that long pastes fold.
    pub(crate) fn paste_aware(
        input_box: gpui::Div,
        input: &Entity<TextareaState>,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        let input = input.clone();
        input_box.capture_action(
            cx.listener(move |this, _: &Paste, window, cx| this.on_paste(&input, window, cx)),
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
                    write_in(&input, format!("/{name} "), window, cx);
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

/// What is left to give of a `/` command's arguments, to show after the
/// text: `None` before the command's name and a space, for a command without
/// an `argument-hint`, or once every argument is typed.
pub(crate) fn argument_hint(typed: &str, commands: &[SlashCommand]) -> Option<String> {
    let (name, args) = typed.strip_prefix('/')?.split_once(char::is_whitespace)?;
    let hint = commands
        .iter()
        .find(|c| c.name == name)?
        .argument_hint
        .as_deref()?;
    let left: Vec<&str> = hint_parts(hint)
        .into_iter()
        .skip(args.split_whitespace().count())
        .collect();
    if left.is_empty() {
        return None;
    }
    let gap = if typed.ends_with(char::is_whitespace) {
        ""
    } else {
        " "
    };
    Some(format!("{gap}{}", left.join(" ")))
}

/// One part per argument: `[environment name] <file> force` gives
/// `[environment name]`, `<file>` and `force`.
fn hint_parts(hint: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut rest = hint.trim_start();
    while !rest.is_empty() {
        let close = match rest.chars().next() {
            Some('[') => rest.find(']'),
            Some('<') => rest.find('>'),
            _ => None,
        };
        let end = close
            .map(|i| i + 1)
            .or_else(|| rest.find(char::is_whitespace))
            .unwrap_or(rest.len());
        parts.push(&rest[..end]);
        rest = rest[end..].trim_start();
    }
    parts
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
        write_in(input, text, window, cx);
        true
    }

    /// The arguments the `/` command in `input` still takes, in grey right
    /// after the caret while the caret is at the end of the text.
    pub(crate) fn argument_ghost(
        &self,
        input: &Entity<TextareaState>,
        project: Option<ProjectId>,
        cx: &App,
    ) -> Option<AnyElement> {
        let state = input.read(cx);
        if state.cursor() != state.text().len() {
            return None;
        }
        let commands = self.snapshot.commands.get(&project?)?;
        let hint = argument_hint(&state.value(), commands)?;
        let input = input.downgrade();
        // Deferred: drawn after the box has placed its caret this frame.
        let ghost = canvas(
            |_, _, _| (),
            move |_, (), window, cx| {
                let Some(caret) = input
                    .upgrade()
                    .and_then(|input| input.read(cx).prepaint_caret_geometry())
                else {
                    return;
                };
                let style = window.text_style();
                let run = TextRun {
                    color: theme::fg_faint().into(),
                    ..style.to_run(hint.len())
                };
                let size = style.font_size.to_pixels(window.rem_size());
                let line = window
                    .text_system()
                    .shape_line(hint.into(), size, &[run], None);
                let bounds = caret.cursor_bounds();
                // The caret is shorter than the line and centred on it.
                let inset = (caret.line_height() - bounds.size.height) / 2.;
                let at = point(
                    bounds.right(),
                    bounds.origin.y + caret.scroll_offset().y - inset,
                );
                let mask = ContentMask {
                    bounds: caret.input_bounds(),
                };
                window.with_content_mask(Some(mask), |window| {
                    let painted =
                        line.paint(at, caret.line_height(), TextAlign::Left, None, window, cx);
                    if let Err(error) = painted {
                        tracing::debug!(%error, "could not draw the argument hint");
                    }
                });
            },
        );
        // The box's text size, out of the box's layout.
        Some(deferred(div().absolute().text_sm().child(ghost.size_0())).into_any_element())
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
        write_in(&self.composer, text.to_owned(), window, cx);
    }
}

/// Puts `text` in `input`, the caret after it.
pub(crate) fn write_in(
    input: &Entity<TextareaState>,
    text: String,
    window: &mut Window,
    cx: &mut App,
) {
    let end = text.len();
    input.update(cx, |state, cx| {
        state.set_value(text, window, cx);
        state.set_selected_range(end..end, cx);
    });
}

/// The message box's placeholder without a suggestion.
pub(crate) const PLACEHOLDER: &str = "Message the agent";

#[cfg(test)]
mod tests {
    use leon_core::ports::SlashCommand;

    use gpui_component::input::InputContent;

    use super::{argument_hint, folds, message, paste_token, slash_completion, slash_matches};

    /// A text with the boxes of `pastes` where `{0}`, `{1}`… stand.
    fn content(text: &str, pastes: &[&str]) -> InputContent {
        let mut full = text.to_owned();
        let mut tokens = Vec::new();
        for (ix, paste) in pastes.iter().enumerate() {
            let token = paste_token(ix, paste);
            let mark = format!("{{{ix}}}");
            let at = full.find(&mark).unwrap();
            full.replace_range(at..at + mark.len(), token.text());
            tokens.push((at..at + token.text().len(), token));
        }
        tokens.sort_by_key(|(range, _)| range.start);
        tokens
            .into_iter()
            .fold(InputContent::new(full), |c, (range, token)| {
                c.with_token(range, token).unwrap()
            })
    }

    #[test]
    fn long_pastes_go_back_where_their_box_is() {
        assert!(!folds("one line"));
        assert!(folds(&"line\n".repeat(10)));
        assert!(folds(&"x".repeat(800)));
        let log = "log line 1\nlog line 2\n";
        let diff = "+a\n-b\n";
        let pasted = [log.to_owned(), diff.to_owned()];
        let cases = [
            (
                content("Why does {0} fail after {1}?", &[log, diff]),
                "Why does log line 1\nlog line 2 fail after +a\n-b?",
            ),
            (content("  {0}  ", &[log]), "log line 1\nlog line 2"),
            (content("hi", &[]), "hi"),
        ];
        for (content, expected) in cases {
            assert_eq!(message(&content, &pasted), expected);
        }
        // A box whose paste is gone keeps its text.
        assert_eq!(message(&content("{0}", &[log]), &[]), "[Pasted text #1]");
        assert_eq!(
            paste_token(0, log).label().as_ref(),
            "Pasted text · 2 lines"
        );
    }

    #[test]
    fn tab_completes_the_first_matching_command() {
        let command = |name: &str| SlashCommand {
            name: name.into(),
            description: String::new(),
            argument_hint: None,
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

    #[test]
    fn argument_hints_fill_in_as_arguments_are_typed() {
        let deploy = SlashCommand {
            name: "deploy".into(),
            description: String::new(),
            argument_hint: Some("[environment name] <version>  force".into()),
        };
        let commands = [deploy];
        let cases = [
            ("/deploy ", Some("[environment name] <version> force")),
            // An argument being typed takes its place.
            ("/deploy prod", Some(" <version> force")),
            ("/deploy prod ", Some("<version> force")),
            ("/deploy prod 1.2 ", Some("force")),
            // Every argument given: nothing left to show.
            ("/deploy prod 1.2 force", None),
            // Only after the full command name and a space.
            ("/deploy", None),
            ("/dep ", None),
            ("deploy ", None),
        ];
        for (typed, expected) in cases {
            assert_eq!(
                argument_hint(typed, &commands).as_deref(),
                expected,
                "{typed:?}"
            );
        }
    }
}
