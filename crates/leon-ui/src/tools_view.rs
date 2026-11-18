//! The Connectors view (TOOLS-1): what each coding CLI can reach — its
//! plugins, its own MCP servers, its account's connectors (Slack, Gmail…) —
//! read from the CLIs; then Leon's shared list of servers, which every CLI
//! gets (add and remove here, I14).

use std::path::PathBuf;

use gpui::{
    AnyElement, Context, Entity, FontWeight, IntoElement, SharedString, Window, div, prelude::*, px,
};
use gpui_component::input::{Input, InputState};
use leon_core::Intent;
use leon_domain::{
    BackendKind, CliConnectors, Connector, ConnectorKind, ConnectorState, McpServer,
    connector_table, split_command,
};

use crate::left::since;
use crate::theme;
use crate::widgets::{OnClick, button, empty, pane_header, primary, segmented};
use crate::workspace::{Workspace, value_of};

/// The add form's inputs and its last error.
pub(crate) struct ToolsForm {
    /// Filters the connectors table by name.
    search: Entity<InputState>,
    /// The table shows every connector, not only the working ones.
    all: bool,
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
            search: input("Find a connector: Slack, Metabase…", window, cx),
            all: false,
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
        let table = match &self.snapshot.tools {
            Some((_, clis)) => self.connectors_table(clis, cx),
            None => empty("Asking each CLI…"),
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
                        .child(table)
                        .child(shared),
                ),
            )
            .into_any_element()
    }

    /// One row per service (Slack, Metabase…), one column per CLI: whether
    /// it works there. Working ones only unless "All" is chosen; a search
    /// narrows by name.
    fn connectors_table(&self, clis: &[CliConnectors], cx: &mut Context<Self>) -> AnyElement {
        let query = value_of(&self.tools_form.search, cx).to_lowercase();
        let all = self.tools_form.all;
        let rows: Vec<_> = connector_table(clis)
            .into_iter()
            .filter(|r| all || r.connected())
            .filter(|r| query.is_empty() || r.name.to_lowercase().contains(&query))
            .collect();
        let backends: Vec<BackendKind> = clis.iter().map(|c| c.backend).collect();
        let filter = segmented(
            "connectors-filter",
            [(false, "Working"), (true, "All")]
                .into_iter()
                .map(|(value, text)| {
                    let view = cx.entity().downgrade();
                    let click: OnClick = Box::new(move |_, _, cx| {
                        view.update(cx, |this, cx| {
                            this.tools_form.all = value;
                            cx.notify();
                        })
                        .ok();
                    });
                    (text.into(), all == value, click)
                })
                .collect(),
        );
        let problems = clis.iter().filter_map(|c| {
            c.problem.as_ref().map(|p| {
                div()
                    .text_size(px(12.))
                    .text_color(theme::warn())
                    .child(format!("{}: {p}", c.backend.display_name()))
            })
        });
        let head = div()
            .flex()
            .items_center()
            .px_4()
            .py_2()
            .border_b_1()
            .border_color(theme::border())
            .text_size(px(12.))
            .text_color(theme::fg_muted())
            .child(div().flex_1().child("Connector"))
            .children(
                backends
                    .iter()
                    .map(|b| div().w(px(COLUMN)).flex_none().child(b.display_name())),
            );
        let body = rows.iter().enumerate().map(|(ix, row)| {
            let kinds: Vec<&str> = {
                let mut k: Vec<&str> = row.entries.iter().map(|(_, c)| kind_name(c.kind)).collect();
                k.sort_unstable();
                k.dedup();
                k
            };
            div()
                .flex()
                .items_center()
                .px_4()
                .py(px(8.))
                .when(ix > 0, |d| d.border_t_1().border_color(theme::border()))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .child(
                            div()
                                .font_weight(FontWeight::MEDIUM)
                                .child(row.name.clone()),
                        )
                        .child(
                            div()
                                .text_size(px(11.5))
                                .text_color(theme::fg_faint())
                                .child(kinds.join(" · ")),
                        ),
                )
                .children(
                    backends
                        .iter()
                        .map(|b| cell(&row.of(*b).collect::<Vec<_>>())),
                )
        });
        let count = rows.len();
        div()
            .flex()
            .flex_col()
            .gap_3()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(div().flex_1().child(Input::new(&self.tools_form.search)))
                    .child(filter),
            )
            .children(problems)
            .child(
                div()
                    .rounded_xl()
                    .border_1()
                    .border_color(theme::border())
                    .bg(theme::panel())
                    .overflow_hidden()
                    .text_size(px(13.))
                    .child(head)
                    .children(body)
                    .when(count == 0, |d| {
                        d.child(empty(if all {
                            "No connector matches."
                        } else {
                            "No working connector matches. Choose All to see the others."
                        }))
                    }),
            )
            .child(hint(
                "Account connectors (Slack, Gmail…) are connected on the CLI's account, e.g. \
                 claude.ai for Claude Code; plugins and servers in each CLI.",
            ))
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

/// Width of a CLI's column in the connectors table.
const COLUMN: f32 = 132.;

fn kind_name(kind: ConnectorKind) -> &'static str {
    match kind {
        ConnectorKind::Plugin => "Plugin",
        ConnectorKind::Server => "Server",
        ConnectorKind::Account => "Account",
    }
}

/// What one CLI has for a service: the best state among its entries, as a
/// dot and a word, or a dash when it has nothing.
fn cell(entries: &[&Connector]) -> gpui::Div {
    let rank = |s: ConnectorState| match s {
        ConnectorState::On => 0,
        ConnectorState::Failed => 1,
        ConnectorState::NeedsLogin => 2,
        ConnectorState::Off => 3,
    };
    let best = entries.iter().map(|c| c.state).min_by_key(|s| rank(*s));
    let (color, text) = match best {
        Some(ConnectorState::On) => (theme::ok(), "connected"),
        Some(ConnectorState::Failed) => (theme::err(), "failing"),
        Some(ConnectorState::NeedsLogin) => (theme::warn(), "log in"),
        Some(ConnectorState::Off) => (theme::fg_faint(), "off"),
        None => (theme::border_strong(), "—"),
    };
    div()
        .w(px(COLUMN))
        .flex_none()
        .flex()
        .items_center()
        .gap(px(6.))
        .text_size(px(12.5))
        .text_color(if best.is_some() {
            theme::fg_muted()
        } else {
            theme::fg_faint()
        })
        .when(best.is_some(), |d| {
            d.child(div().size(px(8.)).rounded_full().bg(color))
        })
        .child(text)
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
