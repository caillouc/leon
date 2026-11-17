//! The Tools view (TOOLS-1): Leon's shared MCP list, which every CLI gets
//! (add and remove here, I14), then what each CLI has on its own (read-only).

use std::path::PathBuf;

use gpui::{
    AnyElement, Context, Entity, FontWeight, IntoElement, SharedString, Window, div, prelude::*, px,
};
use gpui_component::input::{Input, InputState};
use leon_core::Intent;
use leon_domain::{McpServer, split_command};

use crate::doctor::report;
use crate::left::ago;
use crate::theme;
use crate::widgets::{button, empty, label, primary};
use crate::workspace::{Screen, Workspace, value_of};

/// The add form's inputs and its last error.
pub(crate) struct ToolsForm {
    name: Entity<InputState>,
    command: Entity<InputState>,
    env: Entity<InputState>,
    error: Option<String>,
}

impl ToolsForm {
    pub(crate) fn new(window: &mut Window, cx: &mut Context<Workspace>) -> Self {
        let input = |placeholder: &str, window: &mut Window, cx: &mut Context<Workspace>| {
            let placeholder = placeholder.to_owned();
            cx.new(|cx| InputState::new(window, cx).placeholder(placeholder))
        };
        Self {
            name: input("Name, e.g. metabase", window, cx),
            command: input("Command, e.g. npx -y @org/metabase-mcp", window, cx),
            env: input(
                "Environment, e.g. METABASE_URL=https://… API_KEY=…",
                window,
                cx,
            ),
            error: None,
        }
    }
}

/// A server from the form, or what is wrong with it.
pub(crate) fn server_from(name: &str, command: &str, env: &str) -> Result<McpServer, String> {
    let name = name.trim();
    if name.is_empty() || name.contains(char::is_whitespace) {
        return Err("Give the server a name without spaces.".to_owned());
    }
    let words = split_command(command).ok_or("A quote in the command is not closed.")?;
    let (program, args) = words
        .split_first()
        .ok_or("Write the command that starts the server.")?;
    let env = split_command(env)
        .ok_or("A quote in the environment is not closed.")?
        .into_iter()
        .map(|pair| match pair.split_once('=') {
            Some((k, v)) if !k.is_empty() => Ok((k.to_owned(), v.to_owned())),
            _ => Err(format!("\"{pair}\" is not KEY=value.")),
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(McpServer {
        name: name.to_owned(),
        command: PathBuf::from(program),
        args: args.to_vec(),
        env,
        trusted: false,
    })
}

impl Workspace {
    /// The Tools row at the bottom of the left pane.
    pub(crate) fn render_tools_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let selected = self.screen == Screen::Tools;
        let count = self.snapshot.shared_tools.len();
        div()
            .id("tools-row")
            .flex_none()
            .mx_1()
            .px_2()
            .py(px(6.))
            .rounded_md()
            .flex()
            .items_center()
            .gap_2()
            .cursor_pointer()
            .when(selected, |s| s.bg(theme::accent_soft()))
            .when(!selected, |s| s.hover(|s| s.bg(theme::surface_2())))
            .child(div().flex_1().child("Tools"))
            .child(
                div()
                    .text_size(px(11.5))
                    .text_color(theme::fg_faint())
                    .child(format!("{count} shared")),
            )
            .on_click(cx.listener(|this, _, _, cx| {
                this.screen = Screen::Tools;
                this.core.send(Intent::View(None));
                if this.snapshot.tools.is_none() {
                    this.core.send(Intent::RefreshTools);
                }
                cx.notify();
            }))
            .into_any_element()
    }

    pub(crate) fn render_tools(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let running = self.snapshot.tools_running;
        let header = div()
            .px_6()
            .py_3()
            .flex()
            .items_center()
            .gap_3()
            .border_b_1()
            .border_color(theme::border())
            .child(
                div()
                    .text_size(px(15.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("Tools"),
            )
            .children(self.snapshot.tools.as_ref().map(|(at, _)| {
                div()
                    .text_size(px(12.5))
                    .text_color(theme::fg_muted())
                    .child(format!("CLIs checked {} ago", ago(*at)))
            }))
            .child(div().flex_1())
            .child(
                button(
                    "tools-refresh",
                    if running {
                        "Checking…"
                    } else {
                        "Check the CLIs again"
                    },
                    cx.listener(|this, _, _, _| this.core.send(Intent::RefreshTools)),
                )
                .when(running, |b| b.opacity(0.6)),
            );
        let shared = self.render_shared(cx);
        let cli = match &self.snapshot.tools {
            Some((_, sections)) => report(sections),
            None => vec![empty("Asking each CLI…")],
        };
        div()
            .size_full()
            .flex()
            .flex_col()
            .child(header)
            .child(
                div()
                    .id("tools-body")
                    .flex_1()
                    .overflow_y_scroll()
                    .px_6()
                    .py_4()
                    .text_size(px(13.))
                    .child(
                        div()
                            .max_w(px(820.))
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child(shared)
                            .child(label("Each CLI's own (read-only)").px_0().pt_4())
                            .child(hint(
                                "Change these in the CLI itself: claude.ai connectors (Slack, Metabase…) on claude.ai, \
                                 others with `claude mcp …` or `codex mcp …`.",
                            ))
                            .children(cli),
                    ),
            )
            .into_any_element()
    }

    fn render_shared(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let path = self.snapshot.shared_tools_path.display().to_string();
        let rows = self
            .snapshot
            .shared_tools
            .iter()
            .enumerate()
            .map(|(ix, server)| {
                let name = server.name.clone();
                let command = std::iter::once(server.command.display().to_string())
                    .chain(server.args.iter().cloned())
                    .collect::<Vec<_>>()
                    .join(" ");
                let env: Vec<&str> = server.env.iter().map(|(k, _)| k.as_str()).collect();
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .py(px(5.))
                    .border_b_1()
                    .border_color(theme::border())
                    .child(
                        div()
                            .w(px(160.))
                            .flex_none()
                            .font_weight(FontWeight::MEDIUM)
                            .child(name.clone()),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .font_family(theme::MONO)
                                    .text_size(px(12.))
                                    .child(command),
                            )
                            .when(!env.is_empty(), |d| {
                                d.child(
                                    div()
                                        .text_size(px(11.5))
                                        .text_color(theme::fg_faint())
                                        .child(format!("Environment: {}", env.join(", "))),
                                )
                            }),
                    )
                    .child(button(
                        SharedString::from(format!("tool-remove-{ix}")),
                        "Remove",
                        cx.listener(move |this, _, _, _| {
                            this.core.send(Intent::RemoveTool(name.clone()));
                        }),
                    ))
            });
        let form = &self.tools_form;
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(label("Shared by Leon: every CLI gets these").px_0())
            .child(hint(&format!(
                "Saved in {path}. Agent sessions get a change when they start or resume. \
                 Local servers only (a command); add servers that live at a URL in each CLI."
            )))
            .children(self.snapshot.shared_tools_error.as_ref().map(|e| {
                div()
                    .text_color(theme::err())
                    .text_size(px(12.))
                    .child(format!("Could not read the list: {e}"))
            }))
            .children(rows)
            .when(self.snapshot.shared_tools.is_empty(), |d| {
                d.child(empty("No shared server yet."))
            })
            .child(
                div()
                    .mt_2()
                    .p_3()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .rounded_lg()
                    .border_1()
                    .border_color(theme::border())
                    .bg(theme::panel())
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(theme::fg_muted())
                            .child("Add a server"),
                    )
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .child(div().w(px(200.)).child(Input::new(&form.name)))
                            .child(div().flex_1().child(Input::new(&form.command))),
                    )
                    .child(Input::new(&form.env))
                    .children(form.error.as_ref().map(|e| {
                        div()
                            .text_size(px(12.))
                            .text_color(theme::err())
                            .child(e.clone())
                    }))
                    .child(div().flex().justify_end().child(primary(
                        "tool-add",
                        "Add",
                        cx.listener(|this, _, window, cx| this.add_tool(window, cx)),
                    ))),
            )
            .into_any_element()
    }

    fn add_tool(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let form = &self.tools_form;
        match server_from(
            &value_of(&form.name, cx),
            &value_of(&form.command, cx),
            &value_of(&form.env, cx),
        ) {
            Ok(server) => {
                self.core.send(Intent::AddTool(server));
                for input in [&form.name, &form.command, &form.env] {
                    input.update(cx, |state, cx| state.set_value("", window, cx));
                }
                self.tools_form.error = None;
            }
            Err(error) => self.tools_form.error = Some(error),
        }
        cx.notify();
    }
}

fn hint(text: &str) -> gpui::Div {
    div()
        .text_size(px(12.))
        .text_color(theme::fg_faint())
        .child(text.to_owned())
}

#[cfg(test)]
mod tests {
    use super::server_from;

    #[test]
    fn the_form_makes_a_server_or_says_why_not() {
        let server = server_from(
            " metabase ",
            "npx -y @org/metabase-mcp",
            "URL=https://x API_KEY='a b'",
        )
        .unwrap();
        assert_eq!(server.name, "metabase");
        assert_eq!(server.command.to_string_lossy(), "npx");
        assert_eq!(server.args, ["-y", "@org/metabase-mcp"]);
        assert_eq!(
            server.env,
            [
                ("URL".into(), "https://x".into()),
                ("API_KEY".into(), "a b".into())
            ]
        );
        assert!(!server.trusted);
        let cases = [
            ("", "npx x", "", "Give the server a name without spaces."),
            (
                "my server",
                "npx x",
                "",
                "Give the server a name without spaces.",
            ),
            ("m", "", "", "Write the command that starts the server."),
            ("m", "npx \"x", "", "A quote in the command is not closed."),
            ("m", "npx x", "NOPE", "\"NOPE\" is not KEY=value."),
        ];
        for (name, command, env, error) in cases {
            assert_eq!(
                server_from(name, command, env).unwrap_err(),
                error,
                "{name} {command} {env}"
            );
        }
    }
}
