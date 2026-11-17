//! Leon's application services.
//!
//! The core owns the application state and is its only writer. It applies
//! [`Intent`]s from the UI and events from the CLIs, persists what must
//! survive a restart, and publishes [`Snapshot`]s the UI renders.
//!
//! Must not: know a concrete CLI, git, HTTP or database library, or the UI
//! framework. Runs on tokio (P8).

mod agents;
mod app;
mod doctor;
mod events;
mod idle;
mod intent;
pub mod ports;
mod repo;
mod snapshot;
mod state;
mod tools;

pub use app::{CoreHandle, Detached, Ports, detached, start};
pub use doctor::About;
pub use intent::Intent;
pub use snapshot::{
    ConversationView, GitView, Notification, Question, SessionView, Snapshot, TranscriptView,
};

#[cfg(test)]
mod tests;
