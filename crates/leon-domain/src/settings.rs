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
    /// `flow.arrows`: how the flow diagram draws its arrows (FLOW-4).
    pub arrows: ArrowStyle,
}

/// The corners of the flow diagram's arrows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ArrowStyle {
    /// Right angles.
    Square,
    /// Rounded bends.
    #[default]
    Rounded,
}

impl ArrowStyle {
    /// Both styles, in the order the UI offers them.
    pub const ALL: [Self; 2] = [Self::Rounded, Self::Square];

    /// The value in the config file.
    pub fn id(self) -> &'static str {
        match self {
            Self::Square => "square",
            Self::Rounded => "rounded",
        }
    }

    /// The style a config value names.
    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|s| s.id() == id)
    }

    /// Its name in the UI.
    pub fn label(self) -> &'static str {
        match self {
            Self::Square => "Square",
            Self::Rounded => "Rounded",
        }
    }
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
