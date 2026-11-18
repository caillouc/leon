//! The folders of a flow in the right pane (CTX-2): which one
//! Changes, Branch and PR show, picked from a row of chips when there are
//! several.

use std::path::{Path, PathBuf};

use gpui::{AnyElement, Context, FontWeight, IntoElement, div, prelude::*, px};
use leon_core::{GitView, Intent};
use leon_domain::FlowId;

use crate::icons::{Icon, icon};
use crate::pr_view::pr_badge;
use crate::theme;
use crate::workspace::Workspace;

impl Workspace {
    /// The folder Changes, Branch and PR show for flow `id`: the one
    /// picked, else the newest.
    pub(crate) fn folder(&self, id: FlowId) -> Option<PathBuf> {
        let view = self.snapshot.flow(id)?;
        self.right
            .folder
            .get(&id)
            .filter(|f| view.folders.contains(f))
            .or_else(|| view.folders.last())
            .cloned()
    }

    /// The git state of that folder.
    pub(crate) fn folder_git(&self, id: FlowId) -> Option<&GitView> {
        self.snapshot.git.get(&self.folder(id)?)
    }

    /// One chip per folder — repository, branch, its pull request — above
    /// Changes and Branch, so which repository and branch they show is
    /// never a guess; with several, a click shows that folder. The pull
    /// request opens Leon's view of it.
    pub(crate) fn folder_chips(&self, id: FlowId, cx: &mut Context<Self>) -> Option<AnyElement> {
        let view = self.snapshot.flow(id)?;
        if view.folders.is_empty() {
            return None;
        }
        let several = view.folders.len() > 1;
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
                .border_color(if selected && several {
                    theme::accent()
                } else {
                    theme::border()
                })
                .bg(if selected && several {
                    theme::accent_soft()
                } else {
                    theme::window()
                })
                .text_size(px(12.))
                .when(several, |d| {
                    d.cursor_pointer().hover(|s| s.bg(theme::surface_2()))
                })
                .child(icon(Icon::Branch, theme::fg_muted(), 12.))
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
                .children(self.snapshot.prs.get(folder).map(|pr| pr_badge(pr, cx)))
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

impl Workspace {
    /// Info's "Where" (CTX-2): the flow's project, then each folder its
    /// agents work in — repository, branch and what it started from, its
    /// pull request (opening Leon's view of it), its path.
    pub(crate) fn folders_card(
        &self,
        view: &leon_core::FlowView,
        project: Option<&leon_domain::Project>,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        let mono = |text: String, color| {
            div()
                .font_family(theme::MONO)
                .text_size(px(11.))
                .text_color(color)
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .child(text)
        };
        let folder = |path: &PathBuf| {
            let git = self.snapshot.git.get(path);
            div()
                .flex()
                .flex_col()
                .gap(px(3.))
                .pl_2()
                .border_l_2()
                .border_color(theme::accent())
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .gap(px(6.))
                        .text_size(px(12.5))
                        .child(icon(Icon::Branch, theme::accent(), 13.))
                        .child(
                            div()
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(folder_name(path, git)),
                        )
                        .children(
                            git.and_then(|g| g.branch.clone())
                                .map(|b| mono(b, theme::fg_muted())),
                        )
                        .children(self.snapshot.prs.get(path).map(|pr| pr_badge(pr, cx))),
                )
                .children(
                    git.and_then(|g| g.base.as_deref())
                        .map(|b| mono(format!("from {b}"), theme::fg_faint())),
                )
                .child(mono(path.display().to_string(), theme::fg_faint()))
        };
        div()
            .mx_3()
            .px_3()
            .py(px(10.))
            .flex()
            .flex_col()
            .gap_3()
            .rounded_lg()
            .border_1()
            .border_color(theme::border())
            .bg(theme::window())
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .text_size(px(13.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Where"),
                    )
                    .children(project.map(|p| {
                        div()
                            .text_size(px(12.))
                            .text_color(theme::fg_muted())
                            .child(format!("project {}", p.name))
                    })),
            )
            .when(view.folders.is_empty(), |d| {
                d.child(
                    div()
                        .text_size(px(12.))
                        .text_color(theme::fg_faint())
                        .child("No folder yet: one shows once an agent works in it."),
                )
            })
            .children(view.folders.iter().map(folder))
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
