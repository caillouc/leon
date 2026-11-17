//! What each CLI's pickers offer. Values checked on 2026-11-17: Claude Code
//! 2.1.288 `--help` (efforts, permission modes, model aliases), Codex 0.160.0
//! app-server `model/list` (models, efforts).

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
        BackendKind::CopilotCli | BackendKind::Antigravity => BackendOptions::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::codex_policy;

    #[test]
    fn auto_never_asks_and_writes_the_workspace() {
        assert_eq!(codex_policy("auto"), ("never", "workspace-write"));
        assert_eq!(codex_policy("anything else"), ("never", "workspace-write"));
        assert_eq!(codex_policy("read only"), ("on-request", "read-only"));
    }
}
