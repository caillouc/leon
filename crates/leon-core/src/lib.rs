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
mod folders;
mod fresh;
mod held;
mod idle;
mod intent;
mod jobs;
mod links;
mod model_pick;
pub mod ports;
mod repo;
mod settings;
mod snapshot;
mod state;
mod todos;
mod tools;

pub use app::{CoreHandle, Detached, Ports, detached, start};
pub use doctor::About;
pub use intent::Intent;
pub use model_pick::settings_for_agent;
pub use snapshot::{
    Acting, ConversationView, Detail, FlowView, GitView, Notification, PrDiff, Question, Snapshot,
    TranscriptView,
};

#[cfg(test)]
mod tests;
