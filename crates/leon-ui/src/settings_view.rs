//! Settings (SETTINGS-1): the user's config file, edited in the window. Each
//! change is saved at once into `config.toml`, keeping the rest of the file.

use gpui::{AnyElement, Context, Entity, FontWeight, IntoElement, Window, div, prelude::*, px};
use gpui_component::input::{Textarea, TextareaState};
use leon_core::Intent;
use leon_domain::{FoldPattern, SessionDefaults, SessionSettings, Settings};

use crate::icons::{Icon, icon};
use crate::picker::Look;
use crate::theme;
use crate::widgets::{button, pane_header};
use crate::workspace::{Workspace, text_of};

/// The fold patterns box of Settings and its last error.
pub(crate) struct SettingsForm {
    fold: Entity<TextareaState>,
    error: Option<String>,
}

impl SettingsForm {
    pub(crate) fn new(window: &mut Window, cx: &mut Context<Workspace>) -> Self {
        Self {
            fold: cx.new(|cx| {
                TextareaState::new(window, cx)
                    .auto_grow(1, 6)
                    .placeholder("glob:**/*.lock")
            }),
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
        let fold = self.snapshot.settings.diff_fold.join("\n");
        self.settings_form
            .fold
            .update(cx, |s, cx| s.set_value(fold, window, cx));
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
        let path = self
            .snapshot
            .settings_path
            .as_ref()
            .map(|p| p.display().to_string());
        let form = &self.settings_form;
        let save_fold = {
            let settings = settings.clone();
            let fold = form.fold.clone();
            cx.listener(move |this, _, _, cx| {
                let next = Settings {
                    diff_fold: lines(&text_of(&fold, cx)),
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
                "Diffs",
                "Files matching one of these patterns start folded in diffs. One per \
                 line: glob:**/*.lock or re:^generated/.",
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(field(
                        "Fold files",
                        div()
                            .flex()
                            .items_start()
                            .gap_2()
                            .child(div().flex_1().min_w_0().child(Textarea::new(&form.fold)))
                            .child(button("settings-save-fold", "Save", save_fold))
                            .into_any_element(),
                    ))
                    .children(form.error.as_ref().map(|e| {
                        div()
                            .text_size(px(12.))
                            .text_color(theme::err())
                            .child(e.clone())
                    })),
            ))
            .child(section(
                "Source folders",
                "Leon already reads your agents in ~/.agents/agents and \
                 ~/.claude/agents, and each project's .agents, .claude and .github \
                 folders. Add other folders that hold agents, skills, commands or a \
                 .mcp.json; they are read when Leon starts.",
                Self::source_folders(&settings, cx),
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

impl Workspace {
    /// The source folders, each removable, and a button that picks one
    /// with the system's folder dialog.
    fn source_folders(settings: &Settings, cx: &mut Context<Self>) -> AnyElement {
        let rows = settings.sources.iter().enumerate().map(|(ix, folder)| {
            let next = Settings {
                sources: settings
                    .sources
                    .iter()
                    .filter(|f| *f != folder)
                    .cloned()
                    .collect(),
                ..settings.clone()
            };
            div()
                .flex()
                .items_center()
                .gap_2()
                .py(px(5.))
                .when(ix > 0, |d| d.border_t_1().border_color(theme::border()))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .font_family(theme::MONO)
                        .text_size(px(12.5))
                        .child(folder.clone()),
                )
                .child(
                    div()
                        .id(("source-remove", ix))
                        .p(px(3.))
                        .rounded_md()
                        .cursor_pointer()
                        .hover(|s| s.bg(theme::surface_2()))
                        .child(icon(Icon::Close, theme::fg_muted(), 13.))
                        .on_click(cx.listener(move |this, _, _, _| this.save(next.clone()))),
                )
        });
        div()
            .flex()
            .flex_col()
            .gap_2()
            .when(settings.sources.is_empty(), |d| {
                d.child(
                    div()
                        .text_size(px(12.5))
                        .text_color(theme::fg_faint())
                        .child("None yet."),
                )
            })
            .child(div().flex().flex_col().children(rows))
            .child(div().flex().child(button(
                "source-add",
                "Add folder…",
                cx.listener(|_, _, _, cx| Self::pick_source_folder(cx)),
            )))
            .into_any_element()
    }

    /// Asks for a folder with the system's dialog, then adds it to the source
    /// folders.
    fn pick_source_folder(cx: &mut Context<Self>) {
        let paths = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Add source folder".into()),
        });
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = paths.await else {
                return;
            };
            let _ = this.update(cx, |this, _| {
                let mut next = this.snapshot.settings.clone();
                for path in paths {
                    let folder = path.display().to_string();
                    if !next.sources.contains(&folder) {
                        next.sources.push(folder);
                    }
                }
                this.save(next);
            });
        })
        .detach();
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
