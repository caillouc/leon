//! Hands Leon's generic skills (EXT-5) to a CLI that does not read them
//! itself: copies them, in the layout that CLI reads, into a folder of
//! Leon's own (never the CLI's), refreshed each time a session starts. The
//! CLI still loads a skill only when its agent needs it.
//!
//! Copies, not links: Codex 0.160.0 skips a `SKILL.md` that is a symlink
//! (tested 2026-11-18).
//!
//! | CLI | Gets | Layout, then what Leon passes |
//! |---|---|---|
//! | Claude Code | every skill but the source folders' (plugins already, I10) | `<dir>/.claude/skills/<name>/SKILL.md`, `--add-dir <dir>` |
//! | Codex | `<name>.md` skills and the source folders' | `<dir>/<name>/SKILL.md`, `skills/extraRoots/set` |
//! | Copilot CLI | the same | `<dir>/skills/<name>/SKILL.md` and `<dir>/plugin.json`, `--plugin-dir <dir>` |
//!
//! Codex and Copilot CLI read `~/.agents/skills/<name>/SKILL.md` and a
//! project's `.agents/skills/` themselves.

use std::hash::{DefaultHasher, Hash, Hasher};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use leon_core::ports::SkillFile;
use leon_domain::BackendKind;

/// The skills `backend` does not find itself.
fn needed(backend: BackendKind, skill: &SkillFile) -> bool {
    match backend {
        BackendKind::ClaudeCode => !skill.from_source,
        BackendKind::Codex | BackendKind::CopilotCli => skill.from_source || skill.folder.is_none(),
    }
}

/// Writes the skills `backend` needs under `root` and returns the folder
/// to hand it; `None` when it needs none. A skill that cannot be copied is
/// left out (logged).
pub(crate) fn prepare(root: &Path, backend: BackendKind, skills: &[SkillFile]) -> Option<PathBuf> {
    let skills: Vec<&SkillFile> = skills
        .iter()
        .filter(|s| needed(backend, s) && safe_name(&s.name))
        .collect();
    if skills.is_empty() {
        return None;
    }
    // One folder per set of skills: sessions of the same project share it.
    let mut hasher = DefaultHasher::new();
    for skill in &skills {
        (&skill.name, &skill.file).hash(&mut hasher);
    }
    let dir = root
        .join(backend.id())
        .join(format!("{:016x}", hasher.finish()));
    let skills_dir = match backend {
        BackendKind::ClaudeCode => dir.join(".claude").join("skills"),
        BackendKind::Codex => dir.clone(),
        BackendKind::CopilotCli => dir.join("skills"),
    };
    if backend == BackendKind::CopilotCli
        && let Err(error) = write_atomic(&dir.join("plugin.json"), br#"{"name": "leon-skills"}"#)
    {
        tracing::warn!(%error, dir = %dir.display(), "could not prepare the skills for Copilot CLI");
        return None;
    }
    for skill in skills {
        let target = skills_dir.join(&skill.name);
        let copied = match &skill.folder {
            Some(folder) => copy_folder(folder, &target),
            None => std::fs::read(&skill.file)
                .and_then(|text| write_atomic(&target.join("SKILL.md"), &text)),
        };
        if let Err(error) = copied {
            tracing::warn!(%error, skill = skill.name, "could not hand a skill to the CLI");
        }
    }
    Some(dir)
}

/// A name that stays one folder inside the bridge.
fn safe_name(name: &str) -> bool {
    !name.is_empty() && name != "." && name != ".." && !name.contains(['/', '\\'])
}

/// Copies `from` into `to`, file by file, following links.
fn copy_folder(from: &Path, to: &Path) -> io::Result<()> {
    for entry in std::fs::read_dir(from)? {
        let path = entry?.path();
        let Some(name) = path.file_name() else {
            continue;
        };
        if path.is_dir() {
            copy_folder(&path, &to.join(name))?;
        } else {
            write_atomic(&to.join(name), &std::fs::read(&path)?)?;
        }
    }
    Ok(())
}

/// Writes through a temporary file, so a CLI starting at the same time never
/// reads half a file.
fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let temporary = path.with_extension(format!(
        "tmp-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::write(&temporary, bytes)?;
    std::fs::rename(&temporary, path)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use leon_core::ports::SkillFile;
    use leon_domain::BackendKind::{ClaudeCode, Codex, CopilotCli};

    use super::prepare;

    fn files(dir: &Path) -> Vec<String> {
        let mut found = Vec::new();
        for entry in walk(dir) {
            found.push(
                entry
                    .strip_prefix(dir)
                    .unwrap()
                    .to_string_lossy()
                    .into_owned(),
            );
        }
        found.sort();
        found
    }

    fn walk(dir: &Path) -> Vec<std::path::PathBuf> {
        fs::read_dir(dir)
            .unwrap()
            .flat_map(|e| {
                let path = e.unwrap().path();
                if path.is_dir() {
                    walk(&path)
                } else {
                    vec![path]
                }
            })
            .collect()
    }

    #[test]
    fn each_cli_gets_the_skills_it_does_not_read_itself() {
        let home = tempfile::tempdir().unwrap();
        let write = |path: &str, text: &str| {
            let file = home.path().join(path);
            fs::create_dir_all(file.parent().unwrap()).unwrap();
            fs::write(&file, text).unwrap();
            file
        };
        let flat = write(
            ".agents/skills/release.md",
            "---\nname: release\n---\nShip.",
        );
        let folder = write(".agents/skills/pdf/SKILL.md", "---\nname: pdf\n---\nRead.");
        write(".agents/skills/pdf/scripts/read.sh", "echo pdf");
        let source = write("kit/skills/deploy/SKILL.md", "---\nname: deploy\n---\nGo.");
        let skills = [
            SkillFile {
                name: "release".into(),
                file: flat,
                folder: None,
                from_source: false,
            },
            SkillFile {
                name: "pdf".into(),
                file: folder.clone(),
                folder: folder.parent().map(Path::to_path_buf),
                from_source: false,
            },
            SkillFile {
                name: "deploy".into(),
                file: source.clone(),
                folder: source.parent().map(Path::to_path_buf),
                from_source: true,
            },
            SkillFile {
                name: "../escape".into(),
                file: home.path().join("nope.md"),
                folder: None,
                from_source: false,
            },
        ];
        let root = tempfile::tempdir().unwrap();

        let claude = prepare(root.path(), ClaudeCode, &skills).unwrap();
        assert_eq!(
            files(&claude),
            [
                ".claude/skills/pdf/SKILL.md",
                ".claude/skills/pdf/scripts/read.sh",
                ".claude/skills/release/SKILL.md"
            ]
        );
        assert_eq!(
            fs::read_to_string(claude.join(".claude/skills/release/SKILL.md")).unwrap(),
            "---\nname: release\n---\nShip."
        );
        let codex = prepare(root.path(), Codex, &skills).unwrap();
        assert_eq!(files(&codex), ["deploy/SKILL.md", "release/SKILL.md"]);
        let copilot = prepare(root.path(), CopilotCli, &skills).unwrap();
        assert_eq!(
            files(&copilot),
            [
                "plugin.json",
                "skills/deploy/SKILL.md",
                "skills/release/SKILL.md"
            ]
        );
        // The same skills again: the same folder, refreshed.
        assert_eq!(prepare(root.path(), Codex, &skills), Some(codex));
        // Only folders Codex reads itself: nothing to hand.
        assert_eq!(prepare(root.path(), Codex, &skills[1..2]), None);
    }
}
