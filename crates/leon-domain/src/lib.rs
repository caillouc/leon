//! Pure types and rules shared by every Leon crate.
//!
//! Must not: do I/O, use async, or depend on a framework. Words follow the
//! glossary in `AGENTS.md`.

mod backend;
mod conversation;
mod diff;
mod github;
mod project;
mod review;
mod settings;
mod transcript;

pub use backend::BackendKind;
pub use conversation::{
    AgentSession, AgentSessionId, Conversation, ConversationId, SessionSettings, Status,
};
pub use diff::{DiffLine, FileDiff, FileStatus, FoldPattern, Hunk, LineKind, fold_reason};
pub use github::{Check, CheckState, Comment, Issue, PrState, PullRequest};
pub use project::{Project, ProjectId};
pub use review::{ReviewComment, review_message};
pub use settings::{SessionDefaults, Settings};
pub use transcript::{Entry, TimedEntry, Usage};
