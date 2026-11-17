/// A coding CLI Leon drives (glossary: backend). CONV-3.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BackendKind {
    /// Claude Code, binary `claude`.
    ClaudeCode,
    /// Codex CLI, binary `codex`.
    Codex,
    /// GitHub Copilot CLI, binary `copilot`.
    CopilotCli,
    /// Google Antigravity CLI, binary `agy` (P11).
    Antigravity,
}

impl BackendKind {
    /// Every backend, in the order the UI lists them.
    pub const ALL: [Self; 4] = [
        Self::ClaudeCode,
        Self::Codex,
        Self::CopilotCli,
        Self::Antigravity,
    ];

    /// Stable id used in config files and storage.
    pub fn id(self) -> &'static str {
        match self {
            Self::ClaudeCode => "claude",
            Self::Codex => "codex",
            Self::CopilotCli => "copilot",
            Self::Antigravity => "agy",
        }
    }

    /// Finds a backend by its stable id.
    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.id() == id)
    }

    /// Name shown in the UI.
    pub fn display_name(self) -> &'static str {
        match self {
            Self::ClaudeCode => "Claude Code",
            Self::Codex => "Codex",
            Self::CopilotCli => "Copilot CLI",
            Self::Antigravity => "Antigravity",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::BackendKind;

    #[test]
    fn ids_round_trip() {
        for kind in BackendKind::ALL {
            assert_eq!(BackendKind::from_id(kind.id()), Some(kind));
        }
    }

    #[test]
    fn unknown_id_is_none() {
        assert_eq!(BackendKind::from_id("gemini"), None);
        assert_eq!(BackendKind::from_id(""), None);
    }
}
