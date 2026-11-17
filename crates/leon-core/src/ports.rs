//! Traits the adapters implement. Tests use fakes.

use leon_domain::Project;

/// A port call that failed. Says what Leon was doing and why it failed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{context}: {message}")]
pub struct PortError {
    /// What Leon was doing, e.g. "listing projects".
    pub context: &'static str,
    /// The underlying error, as text.
    pub message: String,
}

impl PortError {
    /// Builds an error from any displayable cause.
    pub fn new(context: &'static str, cause: impl std::fmt::Display) -> Self {
        Self {
            context,
            message: cause.to_string(),
        }
    }
}

/// Where projects are stored.
///
/// Implementations may block (disk I/O): the core calls them off the async
/// worker threads.
pub trait ProjectRepository: Send + Sync + 'static {
    /// All projects, sorted by name.
    fn list_projects(&self) -> Result<Vec<Project>, PortError>;
}
