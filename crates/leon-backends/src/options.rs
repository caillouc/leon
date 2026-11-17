//! What each CLI's pickers offer. Values checked on 2026-11-17: Claude Code
//! 2.1.288 `--help` (efforts, permission modes, model aliases), Codex 0.160.0
//! app-server `model/list` (models, efforts); Copilot CLI from its docs only
//! (not run).

use leon_core::ports::BackendOptions;
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

pub(crate) fn for_backend(backend: BackendKind) -> BackendOptions {
    match backend {
        BackendKind::ClaudeCode => BackendOptions {
            models: list(&["opus", "sonnet", "haiku"]),
            efforts: list(&["low", "medium", "high", "xhigh", "max"]),
            permissions: list(&["auto", "bypassPermissions", "acceptEdits", "plan", "manual"]),
        },
        BackendKind::Codex => BackendOptions {
            models: list(&["gpt-6-luna", "gpt-5.6-terra", "gpt-5.6-luna"]),
            efforts: list(&["low", "medium", "high", "xhigh", "max"]),
            permissions: list(&CODEX_PERMISSIONS),
        },
        BackendKind::CopilotCli => BackendOptions {
            // "Supported models" in the command reference; the first is its
            // default. ⚠️ Which ones an account gets is not verified.
            models: list(&[
                "claude-sonnet-5.5",
                "claude-opus-5.5",
                "claude-haiku-5.5",
                "gpt-6.1-sol",
                "gpt-6.1-terra",
                "gpt-6-astra",
                "gpt-6-luna",
                "gpt-5.3-codex",
                "gemini-3.8-flash",
                "auto",
            ]),
            efforts: list(&["low", "medium", "high", "xhigh", "max"]),
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
