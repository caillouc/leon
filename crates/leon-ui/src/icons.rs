//! Icons: Lucide 1.43.0 (ISC licence, `assets/icons/LICENSE-LUCIDE`),
//! embedded in the binary and served to GPUI as its assets. An icon takes the
//! colour of the text around it.

use std::borrow::Cow;

use gpui::{AssetSource, Rgba, SharedString, Svg, prelude::*, px, svg};

/// The icons Leon uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Icon {
    ChevronDown,
    ChevronRight,
    Pending,
    Passed,
    Issue,
    Working,
    Failed,
    External,
    AddProject,
    Branch,
    Merged,
    PullRequest,
    PullRequestClosed,
    PullRequestDraft,
    Info,
    Filter,
    Conversation,
    Copy,
    Connectors,
    Plus,
    Refresh,
    Search,
    Settings,
    NewConversation,
    Doctor,
    Close,
    Panel,
}

impl Icon {
    fn path(self) -> &'static str {
        match self {
            Self::ChevronDown => "icons/chevron-down.svg",
            Self::ChevronRight => "icons/chevron-right.svg",
            Self::Pending => "icons/circle.svg",
            Self::Passed => "icons/circle-check.svg",
            Self::Issue => "icons/circle-dot.svg",
            Self::Working => "icons/loader-circle.svg",
            Self::Failed => "icons/circle-x.svg",
            Self::External => "icons/external-link.svg",
            Self::AddProject => "icons/folder-plus.svg",
            Self::Branch => "icons/git-branch.svg",
            Self::Merged => "icons/git-merge.svg",
            Self::PullRequest => "icons/git-pull-request.svg",
            Self::PullRequestClosed => "icons/git-pull-request-closed.svg",
            Self::PullRequestDraft => "icons/git-pull-request-draft.svg",
            Self::Info => "icons/info.svg",
            Self::Filter => "icons/list-filter.svg",
            Self::Conversation => "icons/message-square.svg",
            Self::Copy => "icons/copy.svg",
            Self::Connectors => "icons/plug.svg",
            Self::Plus => "icons/plus.svg",
            Self::Refresh => "icons/refresh-cw.svg",
            Self::Search => "icons/search.svg",
            Self::Settings => "icons/settings.svg",
            Self::NewConversation => "icons/square-pen.svg",
            Self::Doctor => "icons/stethoscope.svg",
            Self::Close => "icons/x.svg",
            Self::Panel => "icons/panel-right.svg",
        }
    }
}

/// `icon` drawn `size` pixels wide in `color`.
pub(crate) fn icon(icon: Icon, color: Rgba, size: f32) -> Svg {
    svg()
        .path(icon.path())
        .size(px(size))
        .flex_none()
        .text_color(color)
}

/// A fold chevron: pointing down when `open`, right otherwise.
pub(crate) fn chevron(open: bool, color: Rgba, size: f32) -> Svg {
    icon(
        if open {
            Icon::ChevronDown
        } else {
            Icon::ChevronRight
        },
        color,
        size,
    )
}

/// The embedded icons, as GPUI assets.
pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> gpui::Result<Option<Cow<'static, [u8]>>> {
        Ok(bytes(path).map(Cow::Borrowed))
    }

    fn list(&self, path: &str) -> gpui::Result<Vec<SharedString>> {
        Ok(ALL
            .iter()
            .map(|icon| icon.path())
            .filter(|p| p.starts_with(path))
            .map(SharedString::from)
            .collect())
    }
}

const ALL: [Icon; 27] = [
    Icon::ChevronDown,
    Icon::ChevronRight,
    Icon::Pending,
    Icon::Passed,
    Icon::Issue,
    Icon::Working,
    Icon::Failed,
    Icon::External,
    Icon::AddProject,
    Icon::Branch,
    Icon::Merged,
    Icon::PullRequest,
    Icon::PullRequestClosed,
    Icon::PullRequestDraft,
    Icon::Info,
    Icon::Filter,
    Icon::Conversation,
    Icon::Copy,
    Icon::Connectors,
    Icon::Plus,
    Icon::Refresh,
    Icon::Search,
    Icon::Settings,
    Icon::NewConversation,
    Icon::Doctor,
    Icon::Close,
    Icon::Panel,
];

fn bytes(path: &str) -> Option<&'static [u8]> {
    Some(match path {
        "icons/chevron-down.svg" => include_bytes!("../../../assets/icons/chevron-down.svg"),
        "icons/chevron-right.svg" => include_bytes!("../../../assets/icons/chevron-right.svg"),
        "icons/circle.svg" => include_bytes!("../../../assets/icons/circle.svg"),
        "icons/circle-check.svg" => include_bytes!("../../../assets/icons/circle-check.svg"),
        "icons/circle-dot.svg" => include_bytes!("../../../assets/icons/circle-dot.svg"),
        "icons/loader-circle.svg" => include_bytes!("../../../assets/icons/loader-circle.svg"),
        "icons/circle-x.svg" => include_bytes!("../../../assets/icons/circle-x.svg"),
        "icons/external-link.svg" => include_bytes!("../../../assets/icons/external-link.svg"),
        "icons/folder-plus.svg" => include_bytes!("../../../assets/icons/folder-plus.svg"),
        "icons/git-branch.svg" => include_bytes!("../../../assets/icons/git-branch.svg"),
        "icons/git-merge.svg" => include_bytes!("../../../assets/icons/git-merge.svg"),
        "icons/git-pull-request.svg" => {
            include_bytes!("../../../assets/icons/git-pull-request.svg")
        }
        "icons/git-pull-request-closed.svg" => {
            include_bytes!("../../../assets/icons/git-pull-request-closed.svg")
        }
        "icons/git-pull-request-draft.svg" => {
            include_bytes!("../../../assets/icons/git-pull-request-draft.svg")
        }
        "icons/info.svg" => include_bytes!("../../../assets/icons/info.svg"),
        "icons/list-filter.svg" => include_bytes!("../../../assets/icons/list-filter.svg"),
        "icons/message-square.svg" => include_bytes!("../../../assets/icons/message-square.svg"),
        "icons/copy.svg" => include_bytes!("../../../assets/icons/copy.svg"),
        "icons/plug.svg" => include_bytes!("../../../assets/icons/plug.svg"),
        "icons/plus.svg" => include_bytes!("../../../assets/icons/plus.svg"),
        "icons/refresh-cw.svg" => include_bytes!("../../../assets/icons/refresh-cw.svg"),
        "icons/search.svg" => include_bytes!("../../../assets/icons/search.svg"),
        "icons/settings.svg" => include_bytes!("../../../assets/icons/settings.svg"),
        "icons/square-pen.svg" => include_bytes!("../../../assets/icons/square-pen.svg"),
        "icons/stethoscope.svg" => include_bytes!("../../../assets/icons/stethoscope.svg"),
        "icons/x.svg" => include_bytes!("../../../assets/icons/x.svg"),
        "icons/panel-right.svg" => include_bytes!("../../../assets/icons/panel-right.svg"),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::{ALL, bytes};

    #[test]
    fn every_icon_is_embedded() {
        for icon in ALL {
            assert!(
                bytes(icon.path()).is_some_and(|b| b.starts_with(b"<svg")),
                "{icon:?}"
            );
        }
    }
}
