//! What Leon understands in an agent file, in words every CLI shares: the
//! `tools` it may use (EXT-7) and the CLI its `model` picks (EXT-6).

use crate::BackendKind;

/// Something an agent may do, in GitHub's custom-agent tool words.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Capability {
    /// Read files.
    Read,
    /// Search for files or text in files.
    Search,
    /// Create and change files.
    Edit,
    /// Run shell commands.
    Execute,
    /// Fetch web pages, search the web.
    Web,
    /// Start another agent.
    Agent,
    /// Keep a task list.
    Todo,
}

impl Capability {
    /// Every capability.
    pub const ALL: [Self; 7] = [
        Self::Read,
        Self::Search,
        Self::Edit,
        Self::Execute,
        Self::Web,
        Self::Agent,
        Self::Todo,
    ];

    /// A `tools` entry: GitHub's word or a compatible name (Claude Code's
    /// tool names), any case, a `(rule)` after it ignored. `None` for a name
    /// Leon does not know, such as an MCP tool.
    pub fn from_name(name: &str) -> Option<Self> {
        let name = name.split('(').next().unwrap_or_default().trim();
        Some(match name.to_ascii_lowercase().as_str() {
            "read" | "notebookread" => Self::Read,
            "search" | "grep" | "glob" => Self::Search,
            "edit" | "multiedit" | "write" | "notebookedit" => Self::Edit,
            "execute" | "shell" | "bash" | "powershell" => Self::Execute,
            "web" | "websearch" | "webfetch" => Self::Web,
            "agent" | "custom-agent" | "task" => Self::Agent,
            "todo" | "todowrite" => Self::Todo,
            _ => return None,
        })
    }
}

/// The capabilities a `tools` list gives, each once, in [`Capability::ALL`]
/// order; names Leon does not know are left out.
pub fn capabilities<S: AsRef<str>>(tools: &[S]) -> Vec<Capability> {
    let mut found: Vec<Capability> = tools
        .iter()
        .filter_map(|t| Capability::from_name(t.as_ref()))
        .collect();
    found.sort_unstable();
    found.dedup();
    found
}

/// The CLI that runs an agent file's `model`: the first in `order` that
/// offers it, with the model id to give it. `offers` gives a CLI's accepted
/// names (ids and aliases) with the id each one stands for. Dots and hyphens
/// match each other, case is ignored. `None` when no CLI offers it, and for
/// `inherit`.
pub fn cli_for_model(
    model: &str,
    order: &[BackendKind],
    offers: impl Fn(BackendKind) -> Vec<(String, String)>,
) -> Option<(BackendKind, String)> {
    let key = |m: &str| m.trim().to_ascii_lowercase().replace('.', "-");
    let wanted = key(model);
    if wanted.is_empty() || wanted == "inherit" {
        return None;
    }
    order.iter().find_map(|&backend| {
        offers(backend)
            .into_iter()
            .find(|(name, _)| key(name) == wanted)
            .map(|(_, id)| (backend, id))
    })
}

/// `order` with each CLI once, then the CLIs it leaves out, in the default
/// order.
pub fn full_cli_order(order: &[BackendKind]) -> Vec<BackendKind> {
    let mut full: Vec<BackendKind> = Vec::new();
    for backend in order.iter().chain(&BackendKind::ALL) {
        if !full.contains(backend) {
            full.push(*backend);
        }
    }
    full
}

#[cfg(test)]
mod tests {
    use super::{Capability, capabilities, cli_for_model, full_cli_order};
    use crate::BackendKind::{self, ClaudeCode, Codex, CopilotCli};

    #[test]
    fn tools_lists_give_capabilities() {
        use Capability::{Edit, Execute, Read, Search, Web};
        let cases: [(&[&str], Vec<Capability>); 5] = [
            (&[], vec![]),
            (&["read", "search", "execute"], vec![Read, Search, Execute]),
            // Claude Code's names, a rule, an MCP tool, case.
            (
                &[
                    "Read",
                    "Grep",
                    "Glob",
                    "Bash(git *)",
                    "mcp__github__get_pr",
                    "WebFetch",
                ],
                vec![Read, Search, Execute, Web],
            ),
            (&["EDIT", "Write", "MultiEdit"], vec![Edit]),
            (&["something-else"], vec![]),
        ];
        for (tools, expected) in cases {
            assert_eq!(capabilities(tools), expected, "{tools:?}");
        }
    }

    #[test]
    fn a_model_picks_the_first_cli_offering_it() {
        let offers = |backend: BackendKind| -> Vec<(String, String)> {
            let names: &[(&str, &str)] = match backend {
                ClaudeCode => &[
                    ("claude-opus-5-5", "claude-opus-5-5"),
                    ("opus", "claude-opus-5-5"),
                    ("sonnet", "claude-sonnet-5-5"),
                ],
                Codex => &[
                    ("gpt-6-luna", "gpt-6-luna"),
                    ("gpt-5.6-terra", "gpt-5.6-terra"),
                ],
                CopilotCli => &[
                    ("claude-opus-5.5", "claude-opus-5.5"),
                    ("gpt-6-luna", "gpt-6-luna"),
                    ("gemini-3.8-flash", "gemini-3.8-flash"),
                ],
            };
            names
                .iter()
                .map(|(n, id)| ((*n).to_owned(), (*id).to_owned()))
                .collect()
        };
        let default = [ClaudeCode, Codex, CopilotCli];
        let copilot_first = [CopilotCli, ClaudeCode, Codex];
        let pick = |model: &str, order: &[BackendKind]| cli_for_model(model, order, offers);
        let cases = [
            ("opus", &default, Some((ClaudeCode, "claude-opus-5-5"))),
            (
                "Claude-Opus-5.5",
                &default,
                Some((ClaudeCode, "claude-opus-5-5")),
            ),
            ("gpt-6-luna", &default, Some((Codex, "gpt-6-luna"))),
            ("gpt-5-6-terra", &default, Some((Codex, "gpt-5.6-terra"))),
            (
                "gemini-3.8-flash",
                &default,
                Some((CopilotCli, "gemini-3.8-flash")),
            ),
            (
                "claude-opus-5-5",
                &copilot_first,
                Some((CopilotCli, "claude-opus-5.5")),
            ),
            (
                "gpt-6-luna",
                &copilot_first,
                Some((CopilotCli, "gpt-6-luna")),
            ),
            ("llama-9", &default, None),
            ("inherit", &default, None),
            ("", &default, None),
        ];
        for (model, order, expected) in cases {
            let expected = expected.map(|(b, m)| (b, m.to_owned()));
            assert_eq!(pick(model, order), expected, "{model} {order:?}");
        }
    }

    #[test]
    fn the_cli_order_always_has_each_cli_once() {
        let cases: [(&[BackendKind], [BackendKind; 3]); 4] = [
            (&[], [ClaudeCode, Codex, CopilotCli]),
            (&[Codex], [Codex, ClaudeCode, CopilotCli]),
            (
                &[CopilotCli, Codex, CopilotCli],
                [CopilotCli, Codex, ClaudeCode],
            ),
            (
                &[Codex, CopilotCli, ClaudeCode],
                [Codex, CopilotCli, ClaudeCode],
            ),
        ];
        for (order, expected) in cases {
            assert_eq!(full_cli_order(order), expected, "{order:?}");
        }
    }
}
