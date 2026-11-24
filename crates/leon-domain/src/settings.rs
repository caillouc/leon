use crate::BackendKind;

/// The user's settings, independent of the file format they come from.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Settings {
    /// Prefilled choices for a new flow.
    pub defaults: ConversationDefaults,
    /// `diff.fold` patterns (GIT-4), as written.
    pub diff_fold: Vec<String>,
    /// `sources.folders`: the user's agents, skills and MCP servers in the
    /// plugin layout (P3, EXT-2), as written (`~/` allowed).
    pub sources: Vec<String>,
    /// `agents.cli_order`: the CLIs tried, in order, for an agent file's
    /// model (EXT-6), as written; [`crate::full_cli_order`] completes it.
    pub cli_order: Vec<BackendKind>,
    /// `ui.zoom`: the app's zoom (SETTINGS-2).
    pub zoom: Zoom,
    /// `ui.open_flows_on`: what a flow opens on from the left pane
    /// (SETTINGS-3).
    pub open_on: OpenOn,
    /// `ui.editor`: the editor files and worktrees open in (GIT-7); `None`:
    /// the first one installed.
    pub editor: Option<Editor>,
}

/// An editor Leon opens files and worktrees in (GIT-7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Editor {
    /// Visual Studio Code, `code`.
    VsCode,
    /// Zed, `zed`.
    Zed,
}

impl Editor {
    /// Every editor Leon knows, in the order it picks the first installed.
    pub const ALL: [Self; 2] = [Self::VsCode, Self::Zed];

    /// As the config file writes it.
    pub fn id(self) -> &'static str {
        match self {
            Self::VsCode => "vscode",
            Self::Zed => "zed",
        }
    }

    /// From the config file's word.
    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|e| e.id() == id)
    }

    /// Its name in the app.
    pub fn display_name(self) -> &'static str {
        match self {
            Self::VsCode => "VS Code",
            Self::Zed => "Zed",
        }
    }

    /// Where files open: the `chosen` one when installed, else the first
    /// installed.
    pub fn pick(chosen: Option<Self>, installed: &[Self]) -> Option<Self> {
        chosen
            .filter(|e| installed.contains(e))
            .or_else(|| installed.first().copied())
    }
}

/// The app's zoom, in percent: text, icons, spacing and widths
/// (SETTINGS-2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Zoom(pub u16);

impl Zoom {
    /// The sizes Settings offers.
    pub const CHOICES: [u16; 3] = [100, 125, 150];
    /// The sizes the config file may hold.
    pub const RANGE: std::ops::RangeInclusive<u16> = 75..=200;

    /// How much bigger than the default: 1.1 for 110 %.
    pub fn factor(self) -> f32 {
        f32::from(self.0) / 100.
    }
}

impl Default for Zoom {
    fn default() -> Self {
        Self(100)
    }
}

/// What a flow opens on when picked in the left pane (SETTINGS-3); its
/// other tabs stay.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OpenOn {
    /// Its Flow tab, the diagram.
    #[default]
    Flow,
    /// The conversation the user started it with.
    Conversation,
}

impl OpenOn {
    /// As the config file writes it.
    pub fn id(self) -> &'static str {
        match self {
            Self::Flow => "flow",
            Self::Conversation => "conversation",
        }
    }

    /// From the config file's word.
    pub fn from_id(id: &str) -> Option<Self> {
        [Self::Flow, Self::Conversation]
            .into_iter()
            .find(|o| o.id() == id)
    }
}

/// Choices prefilled when the user starts a flow (CONV-3, CONV-4).
///
/// Model, effort and permission are the CLI's own values, so they stay
/// strings; the backend checks them when a conversation starts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConversationDefaults {
    /// CLI to use.
    pub backend: BackendKind,
    /// Agent picked when the project has it; `None`: the CLI's own agent.
    pub agent: Option<String>,
    /// Model id; `None` lets the CLI pick its default.
    pub model: Option<String>,
    /// Effort level; `None` lets the CLI pick its default.
    pub effort: Option<String>,
    /// Permission mode (CONV-4: default auto).
    pub permission: String,
}

impl Default for ConversationDefaults {
    fn default() -> Self {
        Self {
            backend: BackendKind::ClaudeCode,
            agent: None,
            model: None,
            effort: None,
            permission: "auto".to_owned(),
        }
    }
}
