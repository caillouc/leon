//! What each CLI's pickers offer. Values checked on 2026-11-17: Claude Code
//! 2.1.288 `--help` (efforts, permission modes, model aliases), Codex 0.160.0
//! app-server `model/list` (models, efforts); on 2026-11-18, Copilot CLI
//! 1.0.113 over ACP (`session/new` lists its models; which refuse an effort).

use leon_core::ports::{BackendOptions, ModelOption};
use leon_domain::BackendKind;

fn list(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| (*s).to_owned()).collect()
}

/// Permission modes offered for Codex, mapped to app-server settings in
/// [`codex_policy`]. The first one is Leon's "auto" (CONV-4).
pub(crate) const CODEX_PERMISSIONS: [&str; 4] = ["auto", "full access", "on request", "read only"];

/// Codex approval policy and sandbox for a permission mode.
pub(crate) fn codex_policy(permission: &str) -> (&'static str, &'static str) {
    match permission {
        "full access" => ("never", "danger-full-access"),
        "on request" => ("on-request", "workspace-write"),
        "read only" => ("on-request", "read-only"),
        _ => ("never", "workspace-write"),
    }
}

/// Permission modes offered for Copilot CLI, mapped to its flags in
/// [`copilot_flags`]. The first one is Leon's "auto" (CONV-4).
pub(crate) const COPILOT_PERMISSIONS: [&str; 3] = ["auto", "allow all", "ask"];

/// Copilot models whose session refuses `--effort` ("does not support
/// reasoning effort configuration"): every other model accepted one when a
/// session opened (Copilot CLI 1.0.113, 2026-11-18; Haiku 4.5 without an
/// effort answers on 1.0.114).
pub(crate) const COPILOT_NO_EFFORT: [&str; 2] = ["claude-haiku-4.5", "auto"];

/// Copilot CLI flags for a permission mode, from its command reference.
/// ⚠️ not verified that `copilot --acp` honours them: its ACP page names only
/// tool filters, effort and sandbox as server options.
pub(crate) fn copilot_flags(permission: &str) -> &'static [&'static str] {
    match permission {
        "allow all" => &["--allow-all"],
        "ask" => &[],
        _ => &["--allow-all-tools"],
    }
}

/// Models as `(id, label)`.
fn models(items: &[(&str, &str)]) -> Vec<ModelOption> {
    items
        .iter()
        .map(|(id, label)| ModelOption {
            id: (*id).to_owned(),
            label: (*label).to_owned(),
            aliases: Vec::new(),
        })
        .collect()
}

/// Gives `models` their aliases, as `(alias, id)`.
fn with_aliases(mut models: Vec<ModelOption>, aliases: &[(&str, &str)]) -> Vec<ModelOption> {
    for (alias, id) in aliases {
        if let Some(model) = models.iter_mut().find(|m| m.id == *id) {
            model.aliases.push((*alias).to_owned());
        }
    }
    models
}

pub(crate) fn for_backend(backend: BackendKind) -> BackendOptions {
    match backend {
        // Full ids, so the label is what runs. Checked on 2026-11-17 with
        // `--output-format json`: `opus` and the default run claude-opus-5-5,
        // `sonnet` claude-sonnet-5-5, `haiku` claude-haiku-4-5. Claude Code
        // does not report its default effort: Leon uses `high`.
        BackendKind::ClaudeCode => BackendOptions {
            // `fable` is an alias too (`--help`), for a model not listed here.
            models: with_aliases(
                models(&[
                    ("claude-opus-5-5", "Opus 5.5"),
                    ("claude-sonnet-5-5", "Sonnet 5.5"),
                    ("claude-haiku-4-5", "Haiku 4.5"),
                ]),
                &[
                    ("opus", "claude-opus-5-5"),
                    ("sonnet", "claude-sonnet-5-5"),
                    ("haiku", "claude-haiku-4-5"),
                ],
            ),
            efforts: list(&["low", "medium", "high", "xhigh", "max"]),
            default_effort: Some("high".to_owned()),
            // Not `plan` (the maintainer, 2026-11-24, D9).
            permissions: list(&["auto", "bypassPermissions", "acceptEdits", "manual"]),
        },
        // app-server `model/list`, 2026-11-17: gpt-6-luna is the default,
        // `medium` every model's default effort.
        BackendKind::Codex => BackendOptions {
            models: models(&[
                ("gpt-6-luna", "GPT-6-Luna"),
                ("gpt-5.6-terra", "GPT-5.6-Terra"),
                ("gpt-5.6-luna", "GPT-5.6-Luna"),
            ]),
            efforts: list(&["low", "medium", "high", "xhigh", "max"]),
            default_effort: Some("medium".to_owned()),
            permissions: list(&CODEX_PERMISSIONS),
        },
        BackendKind::CopilotCli => BackendOptions {
            // What `session/new` listed over ACP on 2026-11-18 (1.0.113), its
            // default first; the list depends on the account. ⚠️ Its default
            // effort is not reported: Leon uses `medium`.
            models: models(&[
                ("claude-sonnet-5.5", "Claude Sonnet 5.5"),
                ("claude-sonnet-5", "Claude Sonnet 5"),
                ("claude-opus-5.5", "Claude Opus 5.5"),
                ("claude-haiku-5.5", "Claude Haiku 5.5"),
                ("claude-haiku-4.5", "Claude Haiku 4.5"),
                ("gpt-6.2-sol", "GPT-6.2 Sol"),
                ("gpt-6.2-astra", "GPT-6.2 Astra"),
                ("gpt-6.1-sol", "GPT-6.1 Sol"),
                ("gpt-6.1-terra", "GPT-6.1 Terra"),
                ("gpt-6-sol", "GPT-6 Sol"),
                ("gpt-6-luna", "GPT-6 Luna"),
                ("gpt-6-astra", "GPT-6 Astra"),
                ("gpt-5.6-sol", "GPT-5.6 Sol"),
                ("gpt-5.6-luna", "GPT-5.6 Luna"),
                ("gpt-5.3-codex", "GPT-5.3-Codex"),
                ("mai-code-1.1-flash", "MAI-Code-1.1-Flash"),
                ("mai-code-2", "MAI Code 2"),
                ("mai-code-2-flash", "MAI Code 2 Flash"),
                ("gemini-4-carbon", "Gemini 4 Carbon"),
                ("gemini-4-argon", "Gemini 4 Argon"),
                ("gemini-4-hydrogen", "Gemini 4 Hydrogen"),
                ("gemini-3.8-flash", "Gemini 3.8 Flash"),
                ("grok-4.7", "Grok 4.7"),
                ("grok-4.6", "Grok 4.6"),
                ("auto", "Auto"),
            ]),
            efforts: list(&["low", "medium", "high", "xhigh", "max"]),
            default_effort: Some("medium".to_owned()),
            permissions: list(&COPILOT_PERMISSIONS),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::{codex_policy, copilot_flags};

    #[test]
    fn auto_never_asks_and_writes_the_workspace() {
        assert_eq!(codex_policy("auto"), ("never", "workspace-write"));
        assert_eq!(codex_policy("anything else"), ("never", "workspace-write"));
        assert_eq!(codex_policy("read only"), ("on-request", "read-only"));
    }

    #[test]
    fn copilot_auto_allows_tools_and_ask_adds_nothing() {
        assert_eq!(copilot_flags("auto"), ["--allow-all-tools"]);
        assert_eq!(copilot_flags("allow all"), ["--allow-all"]);
        assert_eq!(copilot_flags("ask"), [] as [&str; 0]);
    }
}
