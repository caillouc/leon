use std::path::PathBuf;

/// Identifier of a project in Leon's store.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ProjectId(pub i64);

/// A folder registered in Leon, usually a git repository. Groups
/// conversations in the left pane (NAV-2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Project {
    /// Store id.
    pub id: ProjectId,
    /// Name shown in the left pane.
    pub name: String,
    /// Absolute path of the folder.
    pub path: PathBuf,
}
