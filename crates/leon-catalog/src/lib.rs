//! Finds agent definitions, skills and commands in the user's standard
//! folders (P3, EXT-1, CONV-11).
//!
//! Agents are markdown files with a YAML frontmatter (`name`, `description`)
//! and the instructions as body. Folders, in priority order (a project agent
//! hides a user agent of the same name):
//!
//! - project: `.agents/agents/`, `.claude/agents/`, `.github/agents/*.agent.md`
//! - user: `~/.agents/agents/`, `~/.claude/agents/`
//!
//! Must not: write anything, or depend on another adapter.

mod commands;
mod frontmatter;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use leon_core::ports::{AgentDefinition, Catalog, SlashCommand};

use crate::frontmatter::split;

/// Agent files on disk, relative to a home folder.
#[derive(Debug, Clone)]
pub struct AgentFiles {
    home: PathBuf,
}

impl AgentFiles {
    /// Reads user-level agents under `home`.
    pub fn new(home: PathBuf) -> Self {
        Self { home }
    }
}

const FOLDERS: [&str; 3] = [".agents/agents", ".claude/agents", ".github/agents"];

impl Catalog for AgentFiles {
    fn agents(&self, project: &Path) -> Vec<AgentDefinition> {
        let mut by_name = BTreeMap::new();
        // User folders first, project folders last: later entries win.
        for root in [self.home.as_path(), project] {
            for folder in FOLDERS {
                for agent in read_folder(&root.join(folder)) {
                    by_name.insert(agent.name.clone(), agent);
                }
            }
        }
        by_name.into_values().collect()
    }

    fn commands(&self, project: &Path) -> Vec<SlashCommand> {
        commands::commands(&self.home, project)
    }
}

fn read_folder(dir: &Path) -> Vec<AgentDefinition> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut paths: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|ext| ext == "md"))
        .collect();
    paths.sort();
    paths.iter().filter_map(|p| read_agent(p)).collect()
}

fn read_agent(path: &Path) -> Option<AgentDefinition> {
    let text = std::fs::read_to_string(path).ok()?;
    let (fields, body) = split(&text);
    let stem = path.file_stem()?.to_string_lossy();
    let fallback = stem.strip_suffix(".agent").unwrap_or(&stem).to_owned();
    Some(AgentDefinition {
        name: fields
            .get("name")
            .cloned()
            .filter(|n| !n.is_empty())
            .unwrap_or(fallback),
        description: fields.get("description").cloned().unwrap_or_default(),
        instructions: body.trim().to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use std::fs;

    use leon_core::ports::Catalog;

    use super::AgentFiles;

    #[test]
    fn project_agents_hide_user_agents_of_the_same_name() {
        let home = tempfile::tempdir().unwrap();
        let project = tempfile::tempdir().unwrap();
        fs::create_dir_all(home.path().join(".claude/agents")).unwrap();
        fs::create_dir_all(project.path().join(".github/agents")).unwrap();
        fs::write(
            home.path().join(".claude/agents/planner.md"),
            "---\nname: planner\ndescription: user planner\n---\nPlan.",
        )
        .unwrap();
        fs::write(home.path().join(".claude/agents/notes.txt"), "not an agent").unwrap();
        fs::write(
            project.path().join(".github/agents/planner.agent.md"),
            "---\ndescription: project planner\n---\nPlan here.",
        )
        .unwrap();
        fs::write(
            project.path().join(".github/agents/coder.agent.md"),
            "No frontmatter, just text.",
        )
        .unwrap();

        let agents = AgentFiles::new(home.path().to_path_buf()).agents(project.path());
        let names: Vec<_> = agents.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(names, ["coder", "planner"]);
        assert_eq!(agents[1].description, "project planner");
        assert_eq!(agents[1].instructions, "Plan here.");
        assert_eq!(agents[0].instructions, "No frontmatter, just text.");
    }

    #[test]
    fn missing_folders_give_no_agents() {
        let empty = tempfile::tempdir().unwrap();
        assert_eq!(
            AgentFiles::new(empty.path().to_path_buf()).agents(empty.path()),
            Vec::new()
        );
    }
}
