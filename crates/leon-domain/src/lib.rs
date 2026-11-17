//! Pure types and rules shared by every Leon crate.
//!
//! Must not: do I/O, use async, or depend on a framework. Words follow the
//! glossary in `AGENTS.md`.

mod backend;
mod conversation;
mod project;
mod settings;
mod transcript;

pub use backend::BackendKind;
pub use conversation::{
    AgentSession, AgentSessionId, Conversation, ConversationId, SessionSettings, Status,
};
pub use project::{Project, ProjectId};
pub use settings::{SessionDefaults, Settings};
pub use transcript::{Entry, TimedEntry, Usage};
