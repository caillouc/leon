//! The folders of a conversation in the right pane (CTX-2): which one
//! Changes, Branch and PR show, picked from a row of chips when there are
//! several.

use std::path::{Path, PathBuf};

use gpui::{AnyElement, Context, FontWeight, IntoElement, div, prelude::*, px};
use leon_core::{GitView, Intent};
use leon_domain::{ConversationId, PullRequest};

use crate::github::pr_icon;
use crate::theme;
use crate::workspace::Workspace;

impl Workspace {
    /// The folder Changes, Branch and PR show for conversation `id`: the one
    /// picked, else the newest.
    pub(crate) fn folder(&self, id: ConversationId) -> Option<PathBuf> {
        let view = self.snapshot.conversation(id)?;
        self.right
            .folder
            .get(&id)
            .filter(|f| view.folders.contains(f))
            .or_else(|| view.folders.last())
            .cloned()
    }

    /// The git state of that folder.
    pub(crate) fn folder_git(&self, id: ConversationId) -> Option<&GitView> {
        self.snapshot.git.get(&self.folder(id)?)
    }

    /// The pull request of that folder's branch.
    pub(crate) fn folder_pr(&self, id: ConversationId) -> Option<&PullRequest> {
        self.snapshot.prs.get(&self.folder(id)?)
    }

    /// One chip per folder — repository, branch, pull request state — when
    /// the conversation works in several; a click shows that folder.
    pub(crate) fn folder_chips(
        &self,
        id: ConversationId,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let view = self.snapshot.conversation(id)?;
        if view.folders.len() < 2 {
            return None;
        }
        let picked = self.folder(id);
        let chips = view.folders.iter().enumerate().map(|(ix, folder)| {
            let git = self.snapshot.git.get(folder);
            let selected = picked.as_ref() == Some(folder);
            let pick = folder.clone();
            div()
                .id(("folder-chip", ix))
                .flex()
                .items_center()
                .gap(px(5.))
                .px_2()
                .py(px(3.))
                .rounded_md()
                .border_1()
                .border_color(if selected {
                    theme::accent()
                } else {
                    theme::border()
                })
                .bg(if selected {
                    theme::accent_soft()
                } else {
                    theme::window()
                })
                .text_size(px(12.))
                .cursor_pointer()
                .hover(|s| s.bg(theme::surface_2()))
                .children(
                    self.snapshot
                        .prs
                        .get(folder)
                        .map(|pr| pr_icon(pr.state, 12.)),
                )
                .child(
                    div()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(folder_name(folder, git)),
                )
                .children(git.and_then(|g| g.branch.clone()).map(|branch| {
                    div()
                        .font_family(theme::MONO)
                        .text_size(px(11.))
                        .text_color(theme::fg_muted())
                        .child(branch)
                }))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.right.folder.insert(id, pick.clone());
                    this.core.send(Intent::RefreshGit(id));
                    cx.notify();
                }))
        });
        Some(
            div()
                .flex()
                .flex_none()
                .flex_wrap()
                .gap(px(6.))
                .px_3()
                .py_2()
                .border_b_1()
                .border_color(theme::border())
                .children(chips)
                .into_any_element(),
        )
    }
}

/// A folder's short name: its GitHub repository's name, else the folder's
/// own name.
pub(crate) fn folder_name(folder: &Path, git: Option<&GitView>) -> String {
    git.and_then(|g| g.repo.as_deref())
        .and_then(|repo| repo.rsplit('/').next())
        .map(str::to_owned)
        .or_else(|| {
            folder
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
        })
        .unwrap_or_else(|| folder.display().to_string())
}
