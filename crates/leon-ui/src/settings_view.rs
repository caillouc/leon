//! Settings (SETTINGS-1): the user's config file, edited in the window. Each
//! change is saved at once into `config.toml`, keeping the rest of the file.

use gpui::{AnyElement, Context, Entity, FontWeight, IntoElement, Window, div, prelude::*, px};
use gpui_component::input::{Textarea, TextareaState};
use leon_core::Intent;
use leon_domain::{FoldPattern, SessionDefaults, SessionSettings, Settings};

use crate::picker::Look;
use crate::theme;
use crate::widgets::{button, pane_header};
use crate::workspace::{Workspace, text_of};

/// The text fields of Settings and their last error.
pub(crate) struct SettingsForm {
    fold: Entity<TextareaState>,
    sources: Entity<TextareaState>,
    error: Option<String>,
}

impl SettingsForm {
    pub(crate) fn new(window: &mut Window, cx: &mut Context<Workspace>) -> Self {
        let lines =
            |placeholder: &'static str, window: &mut Window, cx: &mut Context<Workspace>| {
                cx.new(|cx| {
                    TextareaState::new(window, cx)
                        .auto_grow(2, 8)
                        .placeholder(placeholder)
                })
            };
        Self {
            fold: lines("glob:**/*.lock\nre:^generated/", window, cx),
            sources: lines("~/my-agents", window, cx),
            error: None,
        }
    }
}

/// Lines of a text field, trimmed, empty ones left out.
fn lines(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_owned)
        .collect()
}

/// The patterns that do not parse, quoted.
fn bad_patterns(patterns: &[String]) -> Vec<String> {
    patterns
        .iter()
        .filter(|p| FoldPattern::parse(p).is_none())
        .map(|p| format!("\"{p}\""))
        .collect()
}

impl Workspace {
    /// Fills the text fields from the saved settings; called when Settings
    /// opens.
    pub(crate) fn load_settings_form(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let settings = &self.snapshot.settings;
        let (fold, sources) = (settings.diff_fold.join("\n"), settings.sources.join("\n"));
        let form = &self.settings_form;
        form.fold.update(cx, |s, cx| s.set_value(fold, window, cx));
        form.sources
            .update(cx, |s, cx| s.set_value(sources, window, cx));
        self.settings_form.error = None;
    }

    fn save(&self, settings: Settings) {
        self.core.send(Intent::SaveSettings(settings));
    }

    pub(crate) fn render_settings(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let settings = self.snapshot.settings.clone();
        let d = &settings.defaults;
        let current = SessionSettings {
            backend: d.backend,
            agent: None,
            model: d.model.clone(),
            effort: d.effort.clone(),
            permission: d.permission.clone(),
        };
        let pickers = {
            let settings = settings.clone();
            self.setting_pickers(
                Look::Field,
                "settings",
                &current,
                cx,
                move |this, next, _, _| {
                    this.save(Settings {
                        defaults: SessionDefaults {
                            backend: next.backend,
                            model: next.model,
                            effort: next.effort,
                            permission: next.permission,
                        },
                        ..settings.clone()
                    });
                },
            )
        };
        let [cli, model, effort, permission] = pickers;
        let arrows = self.arrow_toggle();
        let path = self
            .snapshot
            .settings_path
            .as_ref()
            .map(|p| p.display().to_string());
        let form = &self.settings_form;
        let lists = {
            let settings = settings.clone();
            let (fold, sources) = (form.fold.clone(), form.sources.clone());
            cx.listener(move |this, _, _, cx| {
                let next = Settings {
                    diff_fold: lines(&text_of(&fold, cx)),
                    sources: lines(&text_of(&sources, cx)),
                    ..settings.clone()
                };
                let bad = bad_patterns(&next.diff_fold);
                if bad.is_empty() {
                    this.settings_form.error = None;
                    this.save(next);
                } else {
                    this.settings_form.error = Some(format!(
                        "Not a glob or a regex: {}. Start each line with glob: or re:.",
                        bad.join(", ")
                    ));
                }
                cx.notify();
            })
        };
        let body = div()
            .max_w(px(760.))
            .w_full()
            .mx_auto()
            .px_6()
            .py_5()
            .flex()
            .flex_col()
            .gap_4()
            .child(section(
                "New conversations",
                "What the new-conversation form starts with.",
                div()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(field("CLI", cli))
                    .child(field("Model", model))
                    .child(field("Effort", effort))
                    .child(field("Permission", permission)),
            ))
            .child(section(
                "Flow diagram",
                "How arrows bend between the boxes.",
                field("Arrows", arrows),
            ))
            .child(section(
                "Diffs and source folders",
                "One per line. Patterns fold matching files in diffs (glob: or re:). \
                 Source folders hold your agents, skills and MCP servers; Leon reads \
                 them when it starts.",
                div()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(field(
                        "Fold files",
                        Textarea::new(&form.fold).into_any_element(),
                    ))
                    .child(field(
                        "Source folders",
                        Textarea::new(&form.sources).into_any_element(),
                    ))
                    .children(form.error.as_ref().map(|e| {
                        div()
                            .text_size(px(12.))
                            .text_color(theme::err())
                            .child(e.clone())
                    }))
                    .child(div().flex().justify_end().child(button(
                        "settings-save-lists",
                        "Save",
                        lists,
                    ))),
            ))
            .children(path.map(|path| {
                div()
                    .text_size(px(12.))
                    .text_color(theme::fg_faint())
                    .child(format!("Saved in {path}. Your comments in it are kept."))
            }));
        div()
            .size_full()
            .flex()
            .flex_col()
            .child(pane_header(
                "Settings",
                self.snapshot
                    .settings_path
                    .as_ref()
                    .map(|p| format!("Saved in {}", p.display())),
            ))
            .child(
                div()
                    .id("settings-body")
                    .flex_1()
                    .overflow_y_scroll()
                    .child(body),
            )
            .into_any_element()
    }
}

/// A titled card with a one-line explanation.
fn section(title: &'static str, about: &'static str, content: impl IntoElement) -> AnyElement {
    div()
        .p_4()
        .flex()
        .flex_col()
        .gap_3()
        .rounded_xl()
        .border_1()
        .border_color(theme::border())
        .bg(theme::panel())
        .child(
            div()
                .flex()
                .flex_col()
                .gap_1()
                .child(
                    div()
                        .text_size(px(14.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(title),
                )
                .child(
                    div()
                        .text_size(px(12.5))
                        .text_color(theme::fg_muted())
                        .child(about),
                ),
        )
        .child(content)
        .into_any_element()
}

/// A label on the left, its control on the right.
fn field(name: &'static str, control: AnyElement) -> AnyElement {
    div()
        .flex()
        .items_start()
        .gap_4()
        .child(
            div()
                .w(px(130.))
                .flex_none()
                .pt(px(5.))
                .text_size(px(13.))
                .text_color(theme::fg_muted())
                .child(name),
        )
        .child(div().flex_1().min_w_0().child(control))
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::{bad_patterns, lines};

    #[test]
    fn lines_and_patterns_are_checked() {
        assert_eq!(lines("  a \n\n b\n"), ["a", "b"]);
        assert_eq!(
            bad_patterns(&lines("glob:**/*.lock\nre:(\nre:^gen/")),
            ["\"re:(\""]
        );
    }
}
