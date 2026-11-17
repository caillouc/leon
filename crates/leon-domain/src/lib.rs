//! Pure types and rules shared by every Leon crate.
//!
//! Must not: do I/O, use async, or depend on a framework. Words follow the
//! glossary in `AGENTS.md`.

mod backend;
mod project;
mod settings;

pub use backend::BackendKind;
pub use project::{Project, ProjectId};
pub use settings::{SessionDefaults, Settings};
