//! Settings (SETTINGS-1): the user's config file, edited in the window. Each
//! change is saved at once into `config.toml`, keeping the rest of the file.

use gpui::{
    AnyElement, Context, Entity, FontWeight, IntoElement, SharedString, Subscription, Window, div,
    prelude::*,
};
use gpui_component::input::{InputEvent, Textarea, TextareaState};
use leon_core::Intent;
use leon_domain::{
    ConversationDefaults, ConversationSettings, FoldPattern, OpenOn, Settings, Zoom,
};

use crate::icons::{Icon, icon};
use crate::picker::Look;
use crate::theme;
use crate::widgets::{OnClick, button, pane_header, primary, segmented};
use crate::workspace::{Workspace, text_of};

/// The fold patterns box of Settings.
pub(crate) struct SettingsForm {
    fold: Entity<TextareaState>,
    /// Redraws Settings as the box changes, so its status follows.
    _changed: Subscription,
}

impl SettingsForm {
    pub(crate) fn new(window: &mut Window, cx: &mut Context<Workspace>) -> Self {
        let fold = cx.new(|cx| {
            TextareaState::new(window, cx)
                .auto_grow(1, 6)
                .placeholder("glob:**/*.lock")
        });
        let changed = cx.subscribe(&fold, |_, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                cx.notify();
            }
        });
        Self {
            fold,
            _changed: changed,
        }
    }
}

/// Where the fold box stands against what is saved.
#[derive(Debug, Clone, PartialEq, Eq)]
enum FoldStatus {
    /// It holds what is saved.
    Saved,
    /// It differs: Save writes it.
    Unsaved,
    /// These lines are neither a glob nor a regex.
    Invalid(Vec<String>),
}

fn fold_status(text: &str, saved: &[String]) -> FoldStatus {
    let patterns = lines(text);
    let bad = bad_patterns(&patterns);
    if !bad.is_empty() {
        FoldStatus::Invalid(bad)
    } else if patterns == saved {
        FoldStatus::Saved
    } else {
        FoldStatus::Unsaved
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
    }

    pub(crate) fn save(&self, settings: Settings) {
        self.core.send(Intent::SaveSettings(settings));
    }

    pub(crate) fn render_settings(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let settings = self.snapshot.settings.clone();
        let d = &settings.defaults;
        let current = ConversationSettings {
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
                        defaults: ConversationDefaults {
                            backend: next.backend,
                            agent: settings.defaults.agent.clone(),
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
        let status = fold_status(&text_of(&form.fold, cx), &settings.diff_fold);
        let save_fold = {
            let settings = settings.clone();
            let fold = form.fold.clone();
            let unsaved = status == FoldStatus::Unsaved;
            cx.listener(move |this, _, _, cx| {
                if unsaved {
                    this.save(Settings {
                        diff_fold: lines(&text_of(&fold, cx)),
                        ..settings.clone()
                    });
                }
            })
        };
        let (status_text, status_color) = match &status {
            FoldStatus::Saved => ("Saved".to_owned(), theme::ok()),
            FoldStatus::Unsaved => ("Not saved yet".to_owned(), theme::warn()),
            FoldStatus::Invalid(bad) => (
                format!(
                    "Not a glob or a regex: {}. Start each line with glob: or re:.",
                    bad.join(", ")
                ),
                theme::err(),
            ),
        };
        let body = div()
            .px_6()
            .py_5()
            .flex()
            .flex_col()
            .gap_4()
            .child(section(
                "Appearance",
                "How big the app is, and what a flow shows first when you pick it in \
                 the left pane: its diagram, or the conversation you started it with. \
                 Its other tabs stay either way.",
                div()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(field("Zoom", Self::zoom_choice(&settings, cx)))
                    .child(field("Open a flow on", Self::open_on_choice(&settings, cx))),
            ))
            .child(section(
                "New flows",
                "What the new-flow form starts with.",
                div()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(field("Agent", self.default_agent_picker(&settings, cx)))
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
                            .child(if status == FoldStatus::Unsaved {
                                primary("settings-save-fold", "Save", save_fold)
                            } else {
                                button("settings-save-fold", "Save", save_fold).opacity(0.5)
                            })
                            .into_any_element(),
                    ))
                    // Saved, not yet, or what is wrong: the box always says.
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(crate::theme::rem(5.))
                            .pl(crate::theme::rem(146.))
                            .text_size(crate::theme::rem(12.))
                            .text_color(status_color)
                            .when(status == FoldStatus::Saved, |d| {
                                d.child(icon(Icon::Passed, status_color, 13.))
                            })
                            .child(status_text),
                    ),
            ))
            .child(section(
                "Agents and skills",
                "The folders Leon reads agents and skills from, and what it found in \
                 each. Each project's .agents, .claude and .github folders are read too. \
                 Add a folder that holds agents, skills, commands or a .mcp.json; it is \
                 read when Leon starts.",
                self.agent_folders(&settings, cx),
            ))
            .child(section(
                "Which CLI runs an agent",
                "When an agent file names a model (model: opus, model: gpt-6-luna…), \
                 Leon starts that agent on the first CLI below that offers the model. \
                 Drag the boxes to change the order. Agents without a model run on the \
                 CLI picked in the new-conversation form.",
                Self::cli_order(&settings, cx),
            ))
            .children(path.map(|path| {
                div()
                    .text_size(crate::theme::rem(12.))
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
                    .child(crate::widgets::centered(760., body)),
            )
            .into_any_element()
    }
}

impl Workspace {
    /// The app's zooms, the one in use filled (SETTINGS-2).
    fn zoom_choice(settings: &Settings, cx: &mut Context<Self>) -> AnyElement {
        let options = Zoom::CHOICES
            .into_iter()
            .map(|size| {
                let next = Settings {
                    zoom: Zoom(size),
                    ..settings.clone()
                };
                let click: OnClick = Box::new(cx.listener(move |this, _, _, _| {
                    this.save(next.clone());
                }));
                (
                    SharedString::from(format!("{size} %")),
                    settings.zoom.0 == size,
                    click,
                )
            })
            .collect();
        // As wide as its options, not the field.
        div()
            .flex()
            .child(segmented("settings-zoom", options))
            .into_any_element()
    }

    /// What a flow opens on from the left pane (SETTINGS-3).
    fn open_on_choice(settings: &Settings, cx: &mut Context<Self>) -> AnyElement {
        let options = [
            (OpenOn::Flow, "Flow diagram"),
            (OpenOn::Conversation, "Main conversation"),
        ]
        .into_iter()
        .map(|(open_on, text)| {
            let next = Settings {
                open_on,
                ..settings.clone()
            };
            let click: OnClick = Box::new(cx.listener(move |this, _, _, _| {
                this.save(next.clone());
            }));
            (SharedString::from(text), settings.open_on == open_on, click)
        })
        .collect();
        div()
            .flex()
            .child(segmented("settings-open-on", options))
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
                        .text_size(crate::theme::rem(14.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(title),
                )
                .child(
                    div()
                        .text_size(crate::theme::rem(12.5))
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
                .w(crate::theme::rem(130.))
                .flex_none()
                .pt(crate::theme::rem(5.))
                .text_size(crate::theme::rem(13.))
                .text_color(theme::fg_muted())
                .child(name),
        )
        .child(div().flex_1().min_w_0().child(control))
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::{FoldStatus, bad_patterns, fold_status, lines};

    #[test]
    fn lines_and_patterns_are_checked() {
        assert_eq!(lines("  a \n\n b\n"), ["a", "b"]);
        assert_eq!(
            bad_patterns(&lines("glob:**/*.lock\nre:(\nre:^gen/")),
            ["\"re:(\""]
        );
    }

    #[test]
    fn the_fold_box_says_whether_it_is_saved() {
        let saved = ["glob:**/*test*.py".to_owned()];
        let cases = [
            ("glob:**/*test*.py\n", FoldStatus::Saved),
            ("glob:**/*test*.py\nglob:dist/**", FoldStatus::Unsaved),
            ("", FoldStatus::Unsaved),
            ("re:(", FoldStatus::Invalid(vec!["\"re:(\"".to_owned()])),
        ];
        for (text, expected) in cases {
            assert_eq!(fold_status(text, &saved), expected, "{text:?}");
        }
    }
}
