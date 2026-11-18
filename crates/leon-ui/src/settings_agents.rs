//! The Settings sections about agents (SETTINGS-1): the folders Leon reads
//! agents and skills from, with what each holds, and the order in which CLIs
//! are tried for an agent file's model (EXT-6).

use gpui::{AnyElement, Context, div, prelude::*, px};
use leon_core::ports::AgentFolder;
use leon_domain::{BackendKind, Settings, full_cli_order};

use crate::icons::{Icon, icon};
use crate::theme;
use crate::widgets::button;
use crate::workspace::Workspace;

impl Workspace {
    /// The folders Leon finds itself, then the ones the user added (each
    /// removable), with what each holds; a button picks one more with the
    /// system's folder dialog.
    pub(crate) fn agent_folders(&self, settings: &Settings, cx: &mut Context<Self>) -> AnyElement {
        let found = self.snapshot.folders.iter().filter(|f| !f.added);
        let found = found.map(|f| row(&f.label, Some(f), None));
        let added = settings.sources.iter().enumerate().map(|(ix, folder)| {
            // As written in the config (`~/kit`) or as Leon read it.
            let holds = self.snapshot.folders.iter().find(|f| {
                f.added && (f.label == *folder || f.label.ends_with(folder.trim_start_matches('~')))
            });
            let next = Settings {
                sources: settings
                    .sources
                    .iter()
                    .filter(|f| *f != folder)
                    .cloned()
                    .collect(),
                ..settings.clone()
            };
            let remove = div()
                .id(("source-remove", ix))
                .p(px(3.))
                .rounded_md()
                .cursor_pointer()
                .hover(|s| s.bg(theme::surface_2()))
                .child(icon(Icon::Close, theme::fg_muted(), 13.))
                .on_click(cx.listener(move |this, _, _, _| this.save(next.clone())))
                .into_any_element();
            row(folder, holds, Some(remove))
        });
        let rows: Vec<AnyElement> = found.chain(added).collect();
        div()
            .flex()
            .flex_col()
            .gap_2()
            .when(rows.is_empty(), |d| {
                d.child(
                    div()
                        .text_size(px(12.5))
                        .text_color(theme::fg_faint())
                        .child("No folder yet: ~/.agents, ~/.claude and ~/.github are read when they exist."),
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

    /// The CLIs in the order they are tried for an agent file's model; each
    /// but the first moves one place up.
    pub(crate) fn cli_order(settings: &Settings, cx: &mut Context<Self>) -> AnyElement {
        let order = full_cli_order(&settings.cli_order);
        let items = order.iter().enumerate().map(|(ix, backend)| {
            let mut up: Vec<BackendKind> = order.clone();
            if ix > 0 {
                up.swap(ix, ix - 1);
            }
            let next = Settings {
                cli_order: up,
                ..settings.clone()
            };
            div()
                .flex()
                .items_center()
                .gap_1()
                .px_2()
                .py(px(3.))
                .rounded_md()
                .border_1()
                .border_color(theme::border_strong())
                .text_size(px(12.5))
                .child(
                    div()
                        .text_color(theme::fg_faint())
                        .child(format!("{}", ix + 1)),
                )
                .child(backend.display_name())
                .when(ix > 0, |d| {
                    d.child(
                        div()
                            .id(("cli-up", ix))
                            .ml_1()
                            .px_1()
                            .rounded_sm()
                            .cursor_pointer()
                            .text_color(theme::fg_muted())
                            .hover(|s| s.bg(theme::surface_2()).text_color(theme::fg()))
                            .child("←")
                            .on_click(cx.listener(move |this, _, _, _| this.save(next.clone()))),
                    )
                })
        });
        div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap_2()
            .children(items)
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

/// One folder: where, what it holds, and its remove button when it has one.
fn row(label: &str, holds: Option<&AgentFolder>, remove: Option<AnyElement>) -> AnyElement {
    let plural = |n: usize, one: &str| format!("{n} {one}{}", if n == 1 { "" } else { "s" });
    let holds = holds.map_or_else(
        || "read when Leon starts".to_owned(),
        |f| {
            format!(
                "{} · {}",
                plural(f.agents, "agent"),
                plural(f.skills, "skill")
            )
        },
    );
    div()
        .flex()
        .items_center()
        .gap_3()
        .py(px(5.))
        .border_b_1()
        .border_color(theme::border())
        .child(
            div()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .font_family(theme::MONO)
                .text_size(px(12.5))
                .child(label.to_owned()),
        )
        .child(
            div()
                .flex_none()
                .text_size(px(12.))
                .text_color(theme::fg_muted())
                .child(holds),
        )
        .children(remove)
        .into_any_element()
}
