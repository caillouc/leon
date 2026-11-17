//! Leon's application services.
//!
//! The core owns the application state and is its only writer. It defines the
//! ports (traits) that adapters implement, and publishes snapshots the UI
//! reads.
//!
//! Must not: know a concrete CLI, git, HTTP or database library, or the UI
//! framework. Runs on tokio (P8).

mod app;
pub mod ports;

pub use app::{CoreHandle, Snapshot, start};
