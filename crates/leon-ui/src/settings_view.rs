//! Settings (SETTINGS-1): the user's config file, edited in the window. Each
//! change is saved at once into `config.toml`, keeping the rest of the file.

use gpui::{AnyElement, Context, FontWeight, IntoElement, SharedString, div, prelude::*};
use leon_core::Intent;
use leon_domain::{ConversationDefaults, ConversationSettings, Editor, OpenOn, Settings, Zoom};

use crate::picker::Look;
use crate::theme;
use crate::widgets::{OnClick, pane_header, segmented};
use crate::workspace::Workspace;

impl Workspace {
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
                "Editor",
                "Where \"Open in …\" opens a worktree, or a file of it, from a diff or \
                 the right pane: one of the editors Leon found installed.",
                field("Open in", self.editor_choice(&settings, cx)),
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
                "Files matching one of these patterns start folded in diffs: a glob \
                 (glob:**/*.lock) or a regex (re:^generated/).",
                self.fold_patterns(&settings, cx),
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

    /// The editor files and worktrees open in (GIT-7): one of those
    /// installed, the first by default.
    fn editor_choice(&self, settings: &Settings, cx: &mut Context<Self>) -> AnyElement {
        let installed = &self.snapshot.editors;
        let Some(current) = Editor::pick(settings.editor, installed) else {
            return div()
                .text_size(crate::theme::rem(12.5))
                .text_color(theme::fg_faint())
                .child("None found: install VS Code or Zed, with its command-line tool")
                .into_any_element();
        };
        let options = installed
            .iter()
            .map(|&editor| {
                let next = Settings {
                    editor: Some(editor),
                    ..settings.clone()
                };
                let click: OnClick = Box::new(cx.listener(move |this, _, _, _| {
                    this.save(next.clone());
                }));
                (
                    SharedString::from(editor.display_name()),
                    current == editor,
                    click,
                )
            })
            .collect();
        div()
            .flex()
            .child(segmented("settings-editor", options))
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
