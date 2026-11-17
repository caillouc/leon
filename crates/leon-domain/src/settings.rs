use crate::BackendKind;

/// The user's settings, independent of the file format they come from.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Settings {
    /// Prefilled choices for a new conversation.
    pub defaults: SessionDefaults,
    /// `diff.fold` patterns (GIT-4), as written.
    pub diff_fold: Vec<String>,
    /// `sources.folders`: the user's agents, skills and MCP servers in the
    /// plugin layout (P3, EXT-2), as written (`~/` allowed).
    pub sources: Vec<String>,
}

/// Choices prefilled when the user starts a conversation (CONV-3, CONV-4).
///
/// Model, effort and permission are the CLI's own values, so they stay
/// strings; the backend checks them when a session starts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionDefaults {
    /// CLI to use.
    pub backend: BackendKind,
    /// Model id; `None` lets the CLI pick its default.
    pub model: Option<String>,
    /// Effort level; `None` lets the CLI pick its default.
    pub effort: Option<String>,
    /// Permission mode (CONV-4: default auto).
    pub permission: String,
}

impl Default for SessionDefaults {
    fn default() -> Self {
        Self {
            backend: BackendKind::ClaudeCode,
            model: None,
            effort: None,
            permission: "auto".to_owned(),
        }
    }
}
