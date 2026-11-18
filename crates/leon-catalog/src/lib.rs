//! Finds agent definitions, skills and commands in the user's standard
//! folders (P3, EXT-1, CONV-11).
//!
//! Agents are markdown files with a YAML frontmatter (`name`, `description`,
//! `model`, `tools`) and the instructions as body (EXT-4). Folders, a later
//! one hiding an agent of the same name in an earlier one:
//!
//! - user: `~/.claude/agents/`, `~/.github/agents/`, `~/.agents/agents/`
//! - source folders the user registered (P3): `agents/`
//! - project: `.claude/agents/`, `.github/agents/`, `.agents/agents/`
//!
//! Must not: write anything, or depend on another adapter.

mod commands;
mod frontmatter;
mod shared;
mod sources;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use leon_core::ports::{AgentDefinition, AgentFolder, Catalog, SkillFile, SlashCommand, Source};
use leon_domain::{DoctorCheck, Health};

pub use shared::SharedFile;

use crate::frontmatter::split;

/// Agent files on disk: the user's, the source folders', the project's.
#[derive(Debug, Clone)]
pub struct AgentFiles {
    home: PathBuf,
    sources: Vec<PathBuf>,
}

impl AgentFiles {
    /// Reads user-level files under `home` and the user's source folders.
    pub fn new(home: PathBuf, sources: Vec<PathBuf>) -> Self {
        Self { home, sources }
    }
}

/// Agent folders at one level; the last one wins a name clash (EXT-4).
const FOLDERS: [&str; 3] = [".claude/agents", ".github/agents", ".agents/agents"];

impl Catalog for AgentFiles {
    fn agents(&self, project: &Path) -> Vec<AgentDefinition> {
        // User folders first, project folders last: later entries win.
        let mut found = Vec::new();
        for folder in FOLDERS {
            found.extend(read_folder(&self.home.join(folder)));
        }
        for source in &self.sources {
            found.extend(read_folder(&source.join("agents")));
        }
        for folder in FOLDERS {
            found.extend(read_folder(&project.join(folder)));
        }
        let by_name: BTreeMap<String, AgentDefinition> =
            found.into_iter().map(|a| (a.name.clone(), a)).collect();
        by_name.into_values().collect()
    }

    fn commands(&self, project: &Path) -> Vec<SlashCommand> {
        commands::commands(&self.home, &self.sources, project)
    }

    fn skills(&self, project: &Path) -> Vec<SkillFile> {
        commands::shared(&self.home, &self.sources, project)
    }

    fn sources(&self) -> Vec<Source> {
        self.sources.iter().map(|dir| sources::read(dir)).collect()
    }

    fn folders(&self) -> Vec<AgentFolder> {
        let found = [".agents", ".claude", ".github"]
            .into_iter()
            .map(|name| {
                (
                    format!("~/{name}"),
                    self.home.join(name),
                    false,
                    name == ".agents",
                )
            })
            .filter(|(_, dir, _, _)| dir.is_dir());
        let added = self
            .sources
            .iter()
            .map(|dir| (dir.display().to_string(), dir.clone(), true, false));
        found
            .chain(added)
            .map(|(label, dir, added, flat)| AgentFolder {
                label,
                added,
                agents: read_folder(&dir.join("agents")).len(),
                skills: commands::count(&dir, flat),
            })
            .collect()
    }

    fn doctor(&self) -> Vec<DoctorCheck> {
        if self.sources.is_empty() {
            return vec![DoctorCheck::new(
                Health::Info,
                "Source folders",
                "none (`[sources] folders` in the config)",
            )];
        }
        self.sources
            .iter()
            .map(|dir| {
                if !dir.is_dir() {
                    return DoctorCheck::new(
                        Health::Problem,
                        "Source folder",
                        format!("{} does not exist", dir.display()),
                    )
                    .fix("Fix `[sources] folders` in Leon's config file");
                }
                let agents = read_folder(&dir.join("agents")).len();
                let skills = commands::count(dir, false);
                let servers = sources::read(dir).mcp_servers.len();
                DoctorCheck::new(
                    Health::Ok,
                    "Source folder",
                    format!(
                        "{}: {agents} agents, {skills} skills and commands, {servers} MCP servers",
                        dir.display()
                    ),
                )
            })
            .collect()
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
        model: fields.get("model").cloned().filter(|m| !m.is_empty()),
        tools: fields
            .get("tools")
            .map(|tools| leon_domain::capabilities(&list(tools))),
    })
}

/// A frontmatter list: `a, b`, `[a, "b"]`, or block items (joined with `, `
/// by the parser).
fn list(text: &str) -> Vec<String> {
    text.trim()
        .trim_start_matches('[')
        .trim_end_matches(']')
        .split(',')
        .map(|item| item.trim().trim_matches(['"', '\'']).to_owned())
        .filter(|item| !item.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use std::fs;

    use leon_core::ports::Catalog;
    use leon_domain::Capability;

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

        fs::create_dir_all(home.path().join(".agents/agents")).unwrap();
        fs::write(
            home.path().join(".agents/agents/reviewer.md"),
            "---\nname: reviewer\ndescription: generic\nmodel: opus\ntools: [read, search, execute]\n---\nReview.",
        )
        .unwrap();
        fs::write(
            home.path().join(".claude/agents/reviewer.md"),
            "---\nname: reviewer\ndescription: claude only\ntools:\n  - Read\n  - mcp__github__get_pr\n---\nOld.",
        )
        .unwrap();

        let agents = AgentFiles::new(home.path().to_path_buf(), Vec::new()).agents(project.path());
        let names: Vec<_> = agents.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(names, ["coder", "planner", "reviewer"]);
        assert_eq!(agents[1].description, "project planner");
        assert_eq!(agents[1].instructions, "Plan here.");
        assert_eq!(agents[0].instructions, "No frontmatter, just text.");
        // No `model` or `tools`: the CLI chosen, with every tool it has.
        assert_eq!(
            (agents[0].model.clone(), agents[0].tools.clone()),
            (None, None)
        );
        // `.agents/agents/` wins over `.claude/agents/`.
        let reviewer = &agents[2];
        assert_eq!(reviewer.description, "generic");
        assert_eq!(reviewer.model.as_deref(), Some("opus"));
        assert_eq!(
            reviewer.tools,
            Some(vec![
                Capability::Read,
                Capability::Search,
                Capability::Execute
            ])
        );
    }

    #[test]
    fn folders_found_and_added_with_what_they_hold() {
        let home = tempfile::tempdir().unwrap();
        let kit = tempfile::tempdir().unwrap();
        let write = |root: &std::path::Path, path: &str| {
            let file = root.join(path);
            fs::create_dir_all(file.parent().unwrap()).unwrap();
            fs::write(file, "---\ndescription: d\n---\n").unwrap();
        };
        write(home.path(), ".agents/agents/planner.md");
        write(home.path(), ".agents/skills/release.md");
        write(home.path(), ".agents/skills/pdf/SKILL.md");
        write(home.path(), ".claude/commands/review.md");
        write(kit.path(), "agents/coder.md");
        write(kit.path(), "skills/deploy/SKILL.md");
        let folders =
            AgentFiles::new(home.path().to_path_buf(), vec![kit.path().to_path_buf()]).folders();
        let got: Vec<_> = folders
            .iter()
            .map(|f| (f.label.as_str(), f.added, f.agents, f.skills))
            .collect();
        let kit_label = kit.path().display().to_string();
        assert_eq!(
            got,
            [
                ("~/.agents", false, 1, 2),
                ("~/.claude", false, 0, 1),
                (kit_label.as_str(), true, 1, 1),
            ]
        );
    }

    #[test]
    fn missing_folders_give_no_agents() {
        let empty = tempfile::tempdir().unwrap();
        assert_eq!(
            AgentFiles::new(empty.path().to_path_buf(), Vec::new()).agents(empty.path()),
            Vec::new()
        );
    }
}
