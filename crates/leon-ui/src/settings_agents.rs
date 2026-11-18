//! The Settings sections about agents (SETTINGS-1): the folders Leon reads
//! agents and skills from, with what each holds, and the order in which CLIs
//! are tried for an agent file's model (EXT-6).

use gpui::{AnyElement, Context, div, prelude::*, px};
use leon_core::ports::AgentFolder;
use leon_domain::{BackendKind, SessionDefaults, Settings, full_cli_order};

use crate::icons::{Icon, icon};
use crate::picker::{Choice, Look};
use crate::theme;
use crate::widgets::button;
use crate::workspace::Workspace;

/// The agent picker's "no agent" choice.
const NO_AGENT: &str = "The CLI's own agent";

impl Workspace {
    /// The agent the new-conversation form starts with, among every
    /// project's agents; picked in a project that has it.
    pub(crate) fn default_agent_picker(
        &self,
        settings: &Settings,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let names: std::collections::BTreeSet<&str> = self
            .snapshot
            .agents
            .values()
            .flatten()
            .map(|a| a.name.as_str())
            .collect();
        let choices: Vec<Choice> = std::iter::once((String::new(), NO_AGENT.to_owned()))
            .chain(names.into_iter().map(|n| (n.to_owned(), n.to_owned())))
            .collect();
        let current = settings.defaults.agent.as_deref().unwrap_or(NO_AGENT);
        let settings = settings.clone();
        self.picker_look(
            Look::Field,
            "settings-agent",
            current,
            choices,
            cx,
            move |this, value, _, _| {
                this.save(Settings {
                    defaults: SessionDefaults {
                        agent: (!value.is_empty()).then_some(value),
                        ..settings.defaults.clone()
                    },
                    ..settings.clone()
                });
            },
        )
    }

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

    /// The CLIs in the order they are tried for an agent file's model, as
    /// boxes the user drags to reorder (EXT-6).
    pub(crate) fn cli_order(settings: &Settings, cx: &mut Context<Self>) -> AnyElement {
        let order = full_cli_order(&settings.cli_order);
        let boxes = order.iter().enumerate().map(|(ix, backend)| {
            let (settings, order) = (settings.clone(), order.clone());
            cli_box(ix, backend.display_name(), false)
                .id(("cli-order", ix))
                .cursor(gpui::CursorStyle::OpenHand)
                .on_drag(DraggedCli(*backend), |dragged, _, _, cx| {
                    cx.new(|_| DragPreview(dragged.0.display_name()))
                })
                .drag_over::<DraggedCli>(|style, _, _, _| {
                    style.border_color(theme::accent()).bg(theme::accent_soft())
                })
                .on_drop(cx.listener(move |this, dragged: &DraggedCli, _, _| {
                    let next = moved(&order, dragged.0, ix);
                    if next != order {
                        this.save(Settings {
                            cli_order: next,
                            ..settings.clone()
                        });
                    }
                }))
        });
        div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap_2()
            .children(boxes)
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

/// A CLI being dragged in the order list.
#[derive(Debug, Clone, Copy)]
struct DraggedCli(BackendKind);

/// What follows the mouse while a CLI is dragged: its box.
struct DragPreview(&'static str);

impl gpui::Render for DragPreview {
    fn render(&mut self, _: &mut gpui::Window, _: &mut Context<Self>) -> impl IntoElement {
        cli_box(0, self.0, true).cursor(gpui::CursorStyle::ClosedHand)
    }
}

/// A CLI's box: its place, a grip, its name.
fn cli_box(ix: usize, name: &str, lifted: bool) -> gpui::Div {
    div()
        .flex()
        .items_center()
        .gap_2()
        .px_3()
        .py(px(6.))
        .rounded_lg()
        .border_1()
        .border_color(if lifted {
            theme::accent()
        } else {
            theme::border_strong()
        })
        .bg(theme::surface())
        .when(lifted, gpui::Styled::shadow_lg)
        .text_size(px(13.))
        .child(div().text_color(theme::fg_faint()).child("⋮⋮"))
        .when(!lifted, |d| {
            d.child(
                div()
                    .size(px(18.))
                    .rounded_full()
                    .bg(theme::surface_2())
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(11.))
                    .text_color(theme::fg_muted())
                    .child(format!("{}", ix + 1)),
            )
        })
        .child(name.to_owned())
}

/// `order` with `backend` moved to place `to`.
fn moved(order: &[BackendKind], backend: BackendKind, to: usize) -> Vec<BackendKind> {
    let mut next: Vec<BackendKind> = order.iter().copied().filter(|b| *b != backend).collect();
    next.insert(to.min(next.len()), backend);
    next
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

#[cfg(test)]
mod tests {
    use leon_domain::BackendKind::{ClaudeCode, Codex, CopilotCli};

    use super::moved;

    #[test]
    fn a_dropped_cli_takes_the_place_it_is_dropped_on() {
        let order = [ClaudeCode, Codex, CopilotCli];
        let cases = [
            (CopilotCli, 0, vec![CopilotCli, ClaudeCode, Codex]),
            (ClaudeCode, 2, vec![Codex, CopilotCli, ClaudeCode]),
            (Codex, 1, vec![ClaudeCode, Codex, CopilotCli]),
            (ClaudeCode, 9, vec![Codex, CopilotCli, ClaudeCode]),
        ];
        for (backend, to, expected) in cases {
            assert_eq!(moved(&order, backend, to), expected, "{backend:?} to {to}");
        }
    }
}
