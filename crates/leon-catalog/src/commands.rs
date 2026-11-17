//! Skills and commands for the `/` list (CONV-11), from the standard folders:
//!
//! - skills (Agent Skills standard): `.agents/skills/<name>/SKILL.md`,
//!   `.claude/skills/<name>/SKILL.md`, `.github/skills/<name>/SKILL.md`
//! - commands (Claude plugin layout): `.claude/commands/<name>.md`
//!
//! in the user's home, then the project (a project entry hides a user entry
//! of the same name).

use std::collections::BTreeMap;
use std::path::Path;

use leon_core::ports::SlashCommand;

use crate::frontmatter::split;

const SKILL_FOLDERS: [&str; 3] = [".agents/skills", ".claude/skills", ".github/skills"];
const COMMAND_FOLDER: &str = ".claude/commands";

pub(crate) fn commands(home: &Path, project: &Path) -> Vec<SlashCommand> {
    let mut by_name = BTreeMap::new();
    for root in [home, project] {
        for folder in SKILL_FOLDERS {
            for c in skills(&root.join(folder)) {
                by_name.insert(c.name.clone(), c);
            }
        }
        for c in command_files(&root.join(COMMAND_FOLDER)) {
            by_name.insert(c.name.clone(), c);
        }
    }
    by_name.into_values().collect()
}

fn sorted_entries(dir: &Path) -> Vec<std::path::PathBuf> {
    let mut paths: Vec<_> = std::fs::read_dir(dir)
        .map_or_default(|entries| entries.filter_map(Result::ok).map(|e| e.path()).collect());
    paths.sort();
    paths
}

fn skills(dir: &Path) -> Vec<SlashCommand> {
    sorted_entries(dir)
        .into_iter()
        .filter_map(|folder| {
            let fallback = folder.file_name()?.to_string_lossy().into_owned();
            read(&folder.join("SKILL.md"), fallback)
        })
        .collect()
}

fn command_files(dir: &Path) -> Vec<SlashCommand> {
    sorted_entries(dir)
        .into_iter()
        .filter(|p| p.extension().is_some_and(|ext| ext == "md"))
        .filter_map(|file| {
            let fallback = file.file_stem()?.to_string_lossy().into_owned();
            read(&file, fallback)
        })
        .collect()
}

/// Name from the frontmatter (else `fallback`), description from the
/// frontmatter (else the first line of the body).
fn read(file: &Path, fallback: String) -> Option<SlashCommand> {
    let text = std::fs::read_to_string(file).ok()?;
    let (fields, body) = split(&text);
    let description = fields.get("description").cloned().unwrap_or_else(|| {
        body.lines()
            .map(str::trim)
            .find(|l| !l.is_empty())
            .unwrap_or_default()
            .chars()
            .take(120)
            .collect()
    });
    Some(SlashCommand {
        name: fields
            .get("name")
            .cloned()
            .filter(|n| !n.is_empty())
            .unwrap_or(fallback),
        description,
    })
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::commands;

    #[test]
    fn skills_and_commands_from_home_and_project() {
        let home = tempfile::tempdir().unwrap();
        let project = tempfile::tempdir().unwrap();
        let write = |root: &std::path::Path, path: &str, text: &str| {
            let file = root.join(path);
            fs::create_dir_all(file.parent().unwrap()).unwrap();
            fs::write(file, text).unwrap();
        };
        write(
            home.path(),
            ".agents/skills/pdf/SKILL.md",
            "---\nname: pdf\ndescription: Read PDFs\n---\nBody",
        );
        write(
            home.path(),
            ".claude/commands/review.md",
            "Review the diff.\nMore.",
        );
        write(
            project.path(),
            ".claude/skills/review/SKILL.md",
            "---\ndescription: Project review\n---\n",
        );
        write(
            project.path(),
            ".claude/skills/empty/notes.txt",
            "no SKILL.md here",
        );
        write(
            project.path(),
            ".claude/commands/readme.txt",
            "not a command",
        );

        let list = commands(home.path(), project.path());
        let got: Vec<(&str, &str)> = list
            .iter()
            .map(|c| (c.name.as_str(), c.description.as_str()))
            .collect();
        assert_eq!(got, [("pdf", "Read PDFs"), ("review", "Project review")]);
    }
}
