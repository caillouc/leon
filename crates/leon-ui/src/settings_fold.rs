//! Settings' Diffs section (SETTINGS-1): the patterns of the files that
//! start folded in diffs, each a chip removed by its cross, a new one added
//! from the box under them (Enter or Add). Each change is saved at once.

use gpui::{
    AnyElement, Context, Entity, IntoElement, SharedString, Subscription, Window, div, prelude::*,
};
use gpui_component::input::{Input, InputEvent, InputState};
use leon_domain::{FoldPattern, Settings};

use crate::icons::{Icon, icon};
use crate::theme;
use crate::widgets::button;
use crate::workspace::{Workspace, value_of};

/// The box adding a fold pattern, and why the last one was refused.
pub(crate) struct SettingsForm {
    fold: Entity<InputState>,
    error: Option<String>,
    /// Enter in the box adds its pattern.
    _enter: Subscription,
}

impl SettingsForm {
    pub(crate) fn new(window: &mut Window, cx: &mut Context<Workspace>) -> Self {
        let fold = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Add a pattern: glob:**/*.lock or re:^generated/")
        });
        let enter = cx.subscribe_in(&fold, window, |this, _, event: &InputEvent, window, cx| {
            if let InputEvent::PressEnter { .. } = event {
                this.add_fold_pattern(window, cx);
            }
        });
        Self {
            fold,
            error: None,
            _enter: enter,
        }
    }
}

/// `patterns` with `text` added; `None` when there is nothing to add
/// (empty, or already there); why not when it is neither a glob nor a
/// regex.
fn added(patterns: &[String], text: &str) -> Result<Option<Vec<String>>, String> {
    let text = text.trim();
    if text.is_empty() || patterns.iter().any(|p| p == text) {
        return Ok(None);
    }
    if FoldPattern::parse(text).is_none() {
        return Err(format!(
            "\"{text}\" is neither a glob nor a regex: start it with glob: or re:."
        ));
    }
    Ok(Some(
        patterns.iter().cloned().chain([text.to_owned()]).collect(),
    ))
}

impl Workspace {
    /// Settings opens with an empty box.
    pub(crate) fn load_settings_form(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.settings_form.error = None;
        self.settings_form
            .fold
            .update(cx, |s, cx| s.set_value("", window, cx));
    }

    /// Adds the box's pattern, saved at once, or says why not.
    fn add_fold_pattern(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let settings = self.snapshot.settings.clone();
        let text = value_of(&self.settings_form.fold, cx);
        match added(&settings.diff_fold, &text) {
            Ok(next) => {
                if let Some(diff_fold) = next {
                    self.save(Settings {
                        diff_fold,
                        ..settings
                    });
                }
                self.settings_form.error = None;
                self.settings_form
                    .fold
                    .update(cx, |s, cx| s.set_value("", window, cx));
            }
            Err(error) => self.settings_form.error = Some(error),
        }
        cx.notify();
    }

    /// The patterns as chips, then the box adding one.
    pub(crate) fn fold_patterns(&self, settings: &Settings, cx: &mut Context<Self>) -> AnyElement {
        let chips = settings.diff_fold.iter().enumerate().map(|(ix, pattern)| {
            let next = Settings {
                diff_fold: settings
                    .diff_fold
                    .iter()
                    .filter(|p| *p != pattern)
                    .cloned()
                    .collect(),
                ..settings.clone()
            };
            div()
                .flex()
                .items_center()
                .gap(theme::rem(4.))
                .pl(theme::rem(8.))
                .pr(theme::rem(3.))
                .py(theme::rem(2.))
                .rounded(theme::rem(4.))
                .bg(theme::surface_2())
                .font_family(theme::MONO)
                .text_size(theme::rem(12.))
                .child(pattern.clone())
                .child(
                    div()
                        .id(SharedString::from(format!("fold-remove-{ix}")))
                        .p(theme::rem(2.))
                        .rounded(theme::rem(3.))
                        .cursor_pointer()
                        .hover(|s| s.bg(theme::border()))
                        .child(icon(Icon::Close, theme::fg_muted(), 11.))
                        .on_click(cx.listener(move |this, _, _, _| this.save(next.clone()))),
                )
        });
        let form = &self.settings_form;
        div()
            .flex()
            .flex_col()
            .gap_3()
            .child(div().flex().flex_wrap().gap_2().children(chips))
            .when(settings.diff_fold.is_empty(), |d| {
                d.child(
                    div()
                        .text_size(theme::rem(12.5))
                        .text_color(theme::fg_faint())
                        .child("No pattern yet: every file starts open."),
                )
            })
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(div().w(theme::rem(420.)).child(Input::new(&form.fold)))
                    .child(button(
                        "fold-add",
                        "Add",
                        cx.listener(|this, _, window, cx| this.add_fold_pattern(window, cx)),
                    )),
            )
            .children(form.error.clone().map(|error| {
                div()
                    .text_size(theme::rem(12.))
                    .text_color(theme::err())
                    .child(error)
            }))
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::added;

    #[test]
    fn a_pattern_is_added_once_and_only_when_valid() {
        let saved = ["glob:**/*.lock".to_owned()];
        assert_eq!(
            added(&saved, " glob:dist/** "),
            Ok(Some(vec![
                "glob:**/*.lock".to_owned(),
                "glob:dist/**".to_owned()
            ]))
        );
        assert_eq!(added(&saved, "glob:**/*.lock"), Ok(None));
        assert_eq!(added(&saved, "  "), Ok(None));
        assert!(added(&saved, "re:(").is_err());
    }
}
