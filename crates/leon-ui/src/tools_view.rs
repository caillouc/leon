//! The Connectors view (TOOLS-1): what each coding CLI can reach — its
//! plugins, its own MCP servers, its account's connectors (Slack, Gmail…) —
//! read from the CLIs; then Leon's shared list of servers, which every CLI
//! gets (add and remove here, I14).

use std::collections::HashSet;
use std::path::PathBuf;

use gpui::{
    AnyElement, Context, Entity, FontWeight, IntoElement, SharedString, Window, div, prelude::*, px,
};
use gpui_component::input::{Input, InputState};
use leon_core::Intent;
use leon_domain::{
    BackendKind, CliConnectors, Connector, ConnectorKind, ConnectorState, McpServer, split_command,
};

use crate::icons::{Icon, icon};
use crate::left::since;
use crate::theme;
use crate::widgets::{button, empty, pane_header, primary};
use crate::workspace::{Workspace, value_of};

/// The add form's inputs and its last error.
pub(crate) struct ToolsForm {
    /// CLIs whose not-connected account connectors are shown.
    pub show_offered: HashSet<BackendKind>,
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
            show_offered: HashSet::new(),
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
    /// The Connectors view: per CLI, what it is connected to; then the
    /// servers Leon gives every CLI.
    pub(crate) fn render_tools(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let running = self.snapshot.tools_running;
        let subtitle = match &self.snapshot.tools {
            Some((at, _)) => format!("What each coding CLI can reach · checked {}", since(*at)),
            None => "What each coding CLI can reach".to_owned(),
        };
        let header = pane_header("Connectors", Some(subtitle)).child(
            button(
                "tools-refresh",
                if running { "Checking…" } else { "Refresh" },
                cx.listener(|this, _, _, _| this.core.send(Intent::RefreshTools)),
            )
            .when(running, |b| b.opacity(0.6)),
        );
        let clis: Vec<AnyElement> = match &self.snapshot.tools {
            Some((_, clis)) => clis.iter().map(|c| self.cli_card(c, cx)).collect(),
            None => vec![empty("Asking each CLI…")],
        };
        let shared = self.render_shared(cx);
        div()
            .size_full()
            .flex()
            .flex_col()
            .child(header)
            .child(
                div().id("tools-body").flex_1().overflow_y_scroll().child(
                    div()
                        .max_w(px(820.))
                        .w_full()
                        .mx_auto()
                        .px_6()
                        .py_5()
                        .flex()
                        .flex_col()
                        .gap_4()
                        .text_size(px(13.))
                        .children(clis)
                        .child(shared),
                ),
            )
            .into_any_element()
    }

    /// One CLI: its working connectors, then the disabled and failing ones;
    /// the account's connectors not logged in stay folded behind a count.
    fn cli_card(&self, cli: &CliConnectors, cx: &mut Context<Self>) -> AnyElement {
        let backend = cli.backend;
        let of = |state| cli.connectors.iter().filter(move |c| c.state == state);
        let on = of(ConnectorState::On).count();
        let offered: Vec<&Connector> = of(ConnectorState::NeedsLogin).collect();
        let open = self.tools_form.show_offered.contains(&backend);
        let rows = of(ConnectorState::On)
            .chain(of(ConnectorState::Failed))
            .chain(of(ConnectorState::Off))
            .map(connector_row);
        div()
            .p_4()
            .flex()
            .flex_col()
            .gap_2()
            .rounded_xl()
            .border_1()
            .border_color(theme::border())
            .bg(theme::panel())
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .text_size(px(14.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(backend.display_name()),
                    )
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(theme::fg_muted())
                            .child(format!("{on} working")),
                    ),
            )
            .children(cli.problem.as_ref().map(|p| {
                div()
                    .text_size(px(12.))
                    .text_color(theme::warn())
                    .child(p.clone())
            }))
            .when(cli.connectors.is_empty() && cli.problem.is_none(), |d| {
                d.child(hint("No plugin, server or connector."))
            })
            .child(div().flex().flex_col().children(rows))
            .when(!offered.is_empty(), |d| {
                let n = offered.len();
                d.child(
                    div()
                        .id(SharedString::from(format!("offered-{}", backend.id())))
                        .flex()
                        .items_center()
                        .gap_2()
                        .py_1()
                        .cursor_pointer()
                        .text_size(px(12.5))
                        .text_color(theme::fg_muted())
                        .hover(|s| s.text_color(theme::fg()))
                        .child(icon(
                            if open {
                                Icon::ChevronDown
                            } else {
                                Icon::ChevronRight
                            },
                            theme::fg_muted(),
                            14.,
                        ))
                        .child(format!(
                            "{n} more offered by your account, not connected — log in to one from the CLI"
                        ))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            let shown = &mut this.tools_form.show_offered;
                            if !shown.remove(&backend) {
                                shown.insert(backend);
                            }
                            cx.notify();
                        })),
                )
                .when(open, |d| {
                    d.child(
                        div()
                            .flex()
                            .flex_col()
                            .children(offered.into_iter().map(connector_row)),
                    )
                })
            })
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
            .child(
                div()
                    .text_size(px(14.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("Your own servers, for every CLI"),
            )
            .child(hint(&format!(
                "For an MCP server you run yourself (a command): Leon gives it to every \
                 agent session, whatever its CLI, so you set it up once. Saved in {path}."
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

/// One connector: a dot for its state, its name, what it is, a detail.
fn connector_row(c: &Connector) -> gpui::Div {
    let (color, state) = match c.state {
        ConnectorState::On => (theme::ok(), "on"),
        ConnectorState::Off => (theme::fg_faint(), "off"),
        ConnectorState::NeedsLogin => (theme::fg_faint(), "not connected"),
        ConnectorState::Failed => (theme::err(), "failing"),
    };
    let kind = match c.kind {
        ConnectorKind::Plugin => "Plugin",
        ConnectorKind::Server => "Server",
        ConnectorKind::Account => "Account",
    };
    div()
        .flex()
        .items_center()
        .gap_3()
        .py(px(6.))
        .border_t_1()
        .border_color(theme::border())
        .child(div().size(px(8.)).flex_none().rounded_full().bg(color))
        .child(
            div()
                .w(px(220.))
                .flex_none()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .font_weight(FontWeight::MEDIUM)
                .child(c.name.clone()),
        )
        .child(
            div()
                .w(px(64.))
                .flex_none()
                .text_size(px(11.5))
                .text_color(theme::fg_faint())
                .child(kind),
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
                .child(c.detail.clone()),
        )
        .child(
            div()
                .flex_none()
                .text_size(px(11.5))
                .text_color(color)
                .child(state),
        )
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
