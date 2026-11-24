//! What the view-model tests build their snapshots from.

use leon_core::FlowView;
use leon_domain::{Flow, FlowId, ProjectId, Status};

/// An idle flow in `project`, without conversations, pin or archive.
pub(crate) fn flow_view(id: FlowId, project: ProjectId) -> FlowView {
    FlowView {
        flow: Flow {
            id,
            project,
            title: "Flow".into(),
            pinned: false,
            archived: None,
            unread: false,
            updated_at: 0,
            folders: Vec::new(),
            links: Vec::new(),
            todo_position: 0,
        },
        status: Status::Idle,
        conversations: Vec::new(),
        folders: Vec::new(),
        issues: Vec::new(),
    }
}
