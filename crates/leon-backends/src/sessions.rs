//! A deleted conversation's session in the CLI's own files (NAV-8). Claude
//! Code 2.1.288 keeps a session as `<id>.jsonl`, with a folder `<id>/` for
//! its subagents and tool results, in `~/.claude/projects/<its folder>/`,
//! and lists sessions from those files alone (checked on disk, 2026-11-23).
//! Codex 0.160.0 and Copilot CLI 1.0.114 also index their sessions in
//! databases of their own (`~/.codex/*.sqlite`, `~/.copilot/session-store.db`):
//! removing their files would leave those indexes wrong, so they stay.

use std::path::Path;

/// Removes Claude Code's session `id` from `projects` (its
/// `~/.claude/projects`): its file and its folder, in whichever project
/// folder holds them.
pub(crate) fn discard_claude(projects: &Path, id: &str) {
    // An id as Claude Code makes them, never a path.
    if id.is_empty() || !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return;
    }
    let Ok(folders) = std::fs::read_dir(projects) else {
        return;
    };
    for folder in folders.filter_map(Result::ok).map(|e| e.path()) {
        let file = folder.join(format!("{id}.jsonl"));
        if file.is_file()
            && let Err(error) = std::fs::remove_file(&file)
        {
            tracing::warn!(%error, session = id, "removing a Claude Code session");
        }
        let dir = folder.join(id);
        if dir.is_dir()
            && let Err(error) = std::fs::remove_dir_all(&dir)
        {
            tracing::warn!(%error, session = id, "removing a Claude Code session's folder");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::discard_claude;

    #[test]
    fn a_claude_session_goes_with_its_folder_and_nothing_else() {
        let projects = tempfile::tempdir().unwrap();
        let (api, web) = (
            projects.path().join("-code-api"),
            projects.path().join("-code-web"),
        );
        let id = "5e603514-c12a-43e2-a89d-74315f9ceec5";
        std::fs::create_dir_all(api.join(id).join("subagents")).unwrap();
        std::fs::write(api.join(format!("{id}.jsonl")), "{}").unwrap();
        std::fs::write(api.join("other.jsonl"), "{}").unwrap();
        std::fs::create_dir_all(&web).unwrap();
        std::fs::write(web.join("kept.jsonl"), "{}").unwrap();
        // Never a path: nothing happens.
        discard_claude(projects.path(), "../-code-web/kept");
        discard_claude(projects.path(), "");
        assert!(web.join("kept.jsonl").exists());
        discard_claude(projects.path(), id);
        assert!(!api.join(format!("{id}.jsonl")).exists());
        assert!(!api.join(id).exists());
        assert!(api.join("other.jsonl").exists());
        assert!(web.join("kept.jsonl").exists());
    }
}
