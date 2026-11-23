//! Pure types and rules shared by every Leon crate.
//!
//! Must not: do I/O, use async, or depend on a framework. Words follow the
//! glossary in `AGENTS.md`.

mod agent_file;
mod backend;
mod connectors;
mod date;
mod diff;
mod doctor;
mod flow;
mod github;
mod limits;
mod mcp;
mod plan;
mod project;
mod review;
mod settings;
mod sprint;
mod todo;
mod transcript;
mod unified_diff;

pub use agent_file::{Capability, capabilities, cli_for_model, full_cli_order};
pub use backend::BackendKind;
pub use connectors::{
    CliConnectors, Connector, ConnectorKind, ConnectorRow, ConnectorState, connector_key,
    connector_table,
};
pub use date::Day;
pub use diff::{
    DiffLine, FileDiff, FileStatus, FoldPattern, Hunk, LineKind, fold_reason, folded_last,
};
pub use doctor::{DoctorCheck, DoctorSection, Health, version_check, version_in};
pub use flow::{
    ARCHIVE_KEPT_MS, Attention, Conversation, ConversationId, ConversationSettings, Flow, FlowId,
    Status,
};
pub use github::{
    BoardOption, Check, CheckState, Comment, GhKind, GhRef, Issue, Label, PrState, PullRequest,
    ReviewThread, issues_by_status, prs_by_repo, repo_name,
};
pub use limits::{LimitWindow, window_name};
pub use mcp::{McpServer, mcp_servers_map, split_command};
pub use plan::{PlanStep, StepStatus};
pub use project::{Project, ProjectId};
pub use review::{ReviewComment, review_message};
pub use settings::{ConversationDefaults, OpenOn, Settings, Zoom};
pub use sprint::{Iteration, current_sprint};
pub use todo::{Todo, TodoId, open_todos, todo_flows};
pub use transcript::{
    Entry, NO_RESULT, RunningTool, TimedEntry, Usage, activity, answers_to, end_unfinished,
    running_tools,
};
pub use unified_diff::parse_diff;
