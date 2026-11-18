//! Skills and commands for the `/` list (CONV-11), and the skills Leon hands
//! to every CLI (EXT-5), from the standard folders:
//!
//! - skills: `.claude/skills/<name>/SKILL.md`, `.github/skills/<name>/SKILL.md`,
//!   and Leon's generic `.agents/skills/`, where a skill is a `<name>.md`
//!   file or a `<name>/SKILL.md` folder
//! - commands (Claude plugin layout): `.claude/commands/<name>.md`
//!
//! in the user's home, then each source folder (`skills/<name>/SKILL.md`,
//! `commands/<name>.md`), then the project: a later entry hides an earlier
//! one of the same name, and `.agents/skills/` comes last at each level.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use leon_core::ports::{SkillFile, SlashCommand};

use crate::frontmatter::split;

/// CLI-specific skill folders, read for the `/` list only.
const CLI_SKILL_FOLDERS: [&str; 2] = [".claude/skills", ".github/skills"];
/// Leon's generic skills (EXT-5).
const AGENTS_SKILLS: &str = ".agents/skills";
const COMMAND_FOLDER: &str = ".claude/commands";

pub(crate) fn commands(home: &Path, sources: &[PathBuf], project: &Path) -> Vec<SlashCommand> {
    let mut found = Vec::new();
    let standard = |root: &Path, found: &mut Vec<SlashCommand>| {
        for folder in CLI_SKILL_FOLDERS {
            found.extend(skills(&root.join(folder), false));
        }
        found.extend(command_files(&root.join(COMMAND_FOLDER)));
        found.extend(skills(&root.join(AGENTS_SKILLS), true));
    };
    standard(home, &mut found);
    for source in sources {
        found.extend(skills(&source.join("skills"), false));
        found.extend(command_files(&source.join("commands")));
    }
    standard(project, &mut found);
    let by_name: BTreeMap<String, SlashCommand> =
        found.into_iter().map(|c| (c.name.clone(), c)).collect();
    by_name.into_values().collect()
}

/// How many skills and commands a folder holds in its `skills/` and
/// `commands/`; `flat` counts `<name>.md` skills too (`~/.agents`).
pub(crate) fn count(folder: &Path, flat: bool) -> usize {
    skills(&folder.join("skills"), flat).len() + command_files(&folder.join("commands")).len()
}

/// The skills every CLI gets (EXT-5): `~/.agents/skills/`, the source
/// folders' `skills/`, the project's `.agents/skills/`; one per name, a
/// later one hiding an earlier one.
pub(crate) fn shared(home: &Path, sources: &[PathBuf], project: &Path) -> Vec<SkillFile> {
    let mut found = skill_files(&home.join(AGENTS_SKILLS), true, false);
    for source in sources {
        found.extend(skill_files(&source.join("skills"), false, true));
    }
    found.extend(skill_files(&project.join(AGENTS_SKILLS), true, false));
    let by_name: BTreeMap<String, SkillFile> =
        found.into_iter().map(|s| (s.name.clone(), s)).collect();
    by_name.into_values().collect()
}

/// The skills in `dir`: `<name>/SKILL.md` folders, and `<name>.md` files when
/// `flat` (Leon's `.agents/skills/`).
fn skill_files(dir: &Path, flat: bool, from_source: bool) -> Vec<SkillFile> {
    sorted_entries(dir)
        .into_iter()
        .filter_map(|path| {
            let (file, folder) = if path.join("SKILL.md").is_file() {
                (path.join("SKILL.md"), Some(path))
            } else if flat && path.is_file() && path.extension().is_some_and(|e| e == "md") {
                (path, None)
            } else {
                return None;
            };
            let fallback = folder
                .as_deref()
                .and_then(Path::file_name)
                .or_else(|| file.file_stem())?
                .to_string_lossy()
                .into_owned();
            let name = read(&file, fallback)?.name;
            Some(SkillFile {
                name,
                file,
                folder,
                from_source,
            })
        })
        .collect()
}

fn sorted_entries(dir: &Path) -> Vec<PathBuf> {
    let mut paths: Vec<_> = std::fs::read_dir(dir)
        .map_or_default(|entries| entries.filter_map(Result::ok).map(|e| e.path()).collect());
    paths.sort();
    paths
}

fn skills(dir: &Path, flat: bool) -> Vec<SlashCommand> {
    skill_files(dir, flat, false)
        .into_iter()
        .filter_map(|skill| read(&skill.file, skill.name))
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
        argument_hint: fields
            .get("argument-hint")
            .cloned()
            .filter(|h| !h.is_empty()),
    })
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::{commands, shared};

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

        let source = tempfile::tempdir().unwrap();
        write(
            source.path(),
            "skills/deploy/SKILL.md",
            "---\ndescription: Deploy\nargument-hint: [environment] [version]\n---\n",
        );
        write(
            home.path(),
            ".agents/skills/release.md",
            "---\ndescription: Cut a release\n---\nSteps.",
        );
        write(
            project.path(),
            ".agents/skills/review.md",
            "---\ndescription: Generic review\n---\n",
        );
        write(home.path(), ".agents/skills/notes.txt", "not a skill");
        let sources = [source.path().to_path_buf()];
        let list = commands(home.path(), &sources, project.path());
        let got: Vec<(&str, &str)> = list
            .iter()
            .map(|c| (c.name.as_str(), c.description.as_str()))
            .collect();
        assert_eq!(
            got,
            [
                ("deploy", "Deploy"),
                ("pdf", "Read PDFs"),
                ("release", "Cut a release"),
                // `.agents/skills/` wins over `.claude/skills/`.
                ("review", "Generic review")
            ]
        );
        // What every CLI gets: `.agents/skills/` and the source folders.
        let handed: Vec<_> = shared(home.path(), &sources, project.path())
            .into_iter()
            .map(|s| (s.name, s.folder.is_some(), s.from_source))
            .collect();
        assert_eq!(
            handed,
            [
                ("deploy".to_owned(), true, true),
                ("pdf".to_owned(), true, false),
                ("release".to_owned(), false, false),
                ("review".to_owned(), false, false),
            ]
        );
        // The arguments a command takes, when its file says so.
        assert_eq!(
            list[0].argument_hint.as_deref(),
            Some("[environment] [version]")
        );
        assert_eq!(list[1].argument_hint, None);
    }
}
