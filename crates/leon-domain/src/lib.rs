//! Pure types and rules shared by every Leon crate.
//!
//! Must not: do I/O, use async, or depend on a framework. Words follow the
//! glossary in `AGENTS.md`.

mod backend;
mod connectors;
mod conversation;
mod diff;
mod doctor;
mod github;
mod mcp;
mod project;
mod review;
mod settings;
mod transcript;

pub use backend::BackendKind;
pub use connectors::{CliConnectors, Connector, ConnectorKind, ConnectorState};
pub use conversation::{
    AgentSession, AgentSessionId, Conversation, ConversationId, SessionSettings, Status,
};
pub use diff::{DiffLine, FileDiff, FileStatus, FoldPattern, Hunk, LineKind, fold_reason};
pub use doctor::{DoctorCheck, DoctorSection, Health, version_check, version_in};
pub use github::{
    BoardOption, Check, CheckState, Comment, GhKind, GhRef, Issue, Iteration, Label, PrState,
    PullRequest, issues_by_status,
};
pub use mcp::{McpServer, mcp_servers_map, split_command};
pub use project::{Project, ProjectId};
pub use review::{ReviewComment, review_message};
pub use settings::{ArrowStyle, SessionDefaults, Settings};
pub use transcript::{Entry, TimedEntry, Usage};
