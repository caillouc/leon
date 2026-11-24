//! The right pane's repository and worktree (CTX-1, CTX-2): one of the
//! flow's folders, picked under the tabs when there are several; Changes and
//! Branch show only it.

use std::path::{Path, PathBuf};

use gpui::{AnyElement, Context, FontWeight, IntoElement, div, prelude::*};
use leon_core::{GitView, Intent};
use leon_domain::FlowId;

use crate::theme;
use crate::workspace::Workspace;

impl Workspace {
    /// The folder Changes, Branch and PR show for flow `id`: the one
    /// picked on this tab, else on an agent's tab the one that agent works
    /// in, else the newest.
    pub(crate) fn folder(&self, id: FlowId) -> Option<PathBuf> {
        let view = self.snapshot.flow(id)?;
        let tab = self.current_conversation();
        let agent = tab
            .and_then(|c| self.snapshot.conversations.get(&c))
            .and_then(|s| s.folder.as_ref());
        self.right
            .folder
            .get(&(id, tab))
            .filter(|f| view.folders.contains(f))
            .or(agent.filter(|f| view.folders.contains(f)))
            .or_else(|| view.folders.last())
            .cloned()
    }

    /// The git state of that folder.
    pub(crate) fn folder_git(&self, id: FlowId) -> Option<&GitView> {
        self.snapshot.git.get(&self.folder(id)?)
    }

    /// The right pane's repository and worktree, under Changes and Branch (CTX-1,
    /// CTX-2), as Zed's two pickers: the repository (one of the flow's), then
    /// which of its worktrees, by branch; each a menu only when there is a
    /// choice; then a button copying the branch. Picking redraws every tab.
    pub(crate) fn repo_bar(&self, id: FlowId, cx: &mut Context<Self>) -> Option<AnyElement> {
        let view = self.snapshot.flow(id)?;
        let folder = self.folder(id)?;
        let name = |f: &Path| folder_name(f, self.snapshot.git.get(f));
        let label = |f: &Path| worktree_label(f, self.snapshot.git.get(f));
        let current = name(&folder);
        // The flow's repositories, each once, in the order first worked in;
        // a pick shows the newest worktree of it.
        let mut repos: Vec<(String, PathBuf)> = Vec::new();
        for f in &view.folders {
            match repos.iter_mut().find(|(n, _)| *n == name(f)) {
                Some(repo) => repo.1.clone_from(f),
                None => repos.push((name(f), f.clone())),
            }
        }
        let pick = move |this: &mut Self, value: String| {
            let tab = this.current_conversation();
            this.right.folder.insert((id, tab), PathBuf::from(value));
            this.core.send(Intent::RefreshGit(id));
        };
        let repo: AnyElement = if repos.len() > 1 {
            let choices = repos
                .iter()
                .map(|(n, f)| (f.display().to_string(), n.clone()))
                .collect();
            self.picker_look(
                crate::picker::Look::Title,
                "right-repo",
                &current,
                choices,
                cx,
                move |this, value, _, _| pick(this, value),
            )
        } else {
            div()
                .min_w_0()
                .px_1()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .text_size(crate::theme::rem(13.))
                .font_weight(FontWeight::SEMIBOLD)
                .child(current.clone())
                .into_any_element()
        };
        let worktrees: Vec<&PathBuf> = view.folders.iter().filter(|f| name(f) == current).collect();
        let worktree: AnyElement = if worktrees.len() > 1 {
            let choices = worktrees
                .iter()
                .map(|f| (f.display().to_string(), label(f)))
                .collect();
            self.picker_look(
                crate::picker::Look::Branch,
                "right-worktree",
                &label(&folder),
                choices,
                cx,
                move |this, value, _, _| pick(this, value),
            )
        } else {
            div()
                .min_w_0()
                .px_1()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .font_family(theme::MONO)
                .text_size(crate::theme::rem(11.5))
                .text_color(theme::fg_muted())
                .child(label(&folder))
                .into_any_element()
        };
        let branch = self.folder_git(id).and_then(|g| g.branch.clone());
        let open = self.open_in_editor(folder.clone(), cx);
        Some(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .items_center()
                .gap(crate::theme::rem(4.))
                .child(crate::icons::folder(false, theme::fg_muted(), 13.))
                .child(div().flex_none().max_w(gpui::relative(0.45)).child(repo))
                // The branch shrinks first, its buttons always show.
                .child(div().flex_1().min_w_0().child(worktree))
                .children(branch.map(|b| crate::clipboard::copy_button("right-copy-branch", b)))
                .children(open)
                .into_any_element(),
        )
    }
}

impl Workspace {
    /// "Open in Zed" at the end of the worktree line (GIT-7): the external
    /// link icon and its words while the pane has room, the icon alone when
    /// it is narrow; `None` when no editor is installed.
    fn open_in_editor(&self, folder: PathBuf, cx: &mut Context<Self>) -> Option<AnyElement> {
        /// From this width the pane says the words too.
        const WORDS_FROM: f32 = 400.;
        let editor =
            leon_domain::Editor::pick(self.snapshot.settings.editor, &self.snapshot.editors)?;
        let words = (self.panes.right >= WORDS_FROM)
            .then(|| gpui::SharedString::from(format!("Open in {}", editor.display_name())));
        let open = cx.listener(move |this, _, _, _| {
            this.core.send(Intent::OpenInEditor {
                folder: folder.clone(),
                file: None,
            });
        });
        Some(
            crate::widgets::icon_button(
                "right-open-editor",
                crate::icons::Icon::External,
                words,
                open,
            )
            .into_any_element(),
        )
    }

    /// "Open this worktree in Zed", in the diff drawer's header (GIT-7);
    /// `None` when no editor is installed.
    pub(crate) fn open_worktree(&self, id: FlowId, cx: &mut Context<Self>) -> Option<AnyElement> {
        let folder = self.folder(id)?;
        let editor =
            leon_domain::Editor::pick(self.snapshot.settings.editor, &self.snapshot.editors)?;
        let open = cx.listener(move |this, _, _, _| {
            this.core.send(Intent::OpenInEditor {
                folder: folder.clone(),
                file: None,
            });
        });
        Some(
            crate::widgets::text_action(
                "right-open-editor",
                format!("Open this worktree in {}", editor.display_name()),
                open,
            )
            .into_any_element(),
        )
    }
}

/// How the right pane names a worktree: its branch; the repository's own
/// checkout, "main · <branch>" (CTX-2).
fn worktree_label(folder: &Path, git: Option<&GitView>) -> String {
    let branch = git.and_then(|g| g.branch.clone());
    match (git.is_some_and(|g| g.main), branch) {
        (true, Some(branch)) => format!("main · {branch}"),
        (true, None) => "main · detached".to_owned(),
        (false, Some(branch)) => branch,
        (false, None) => folder.file_name().map_or_else(
            || folder.display().to_string(),
            |n| n.to_string_lossy().into_owned(),
        ),
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

#[cfg(test)]
mod tests {
    use std::path::Path;

    use leon_core::GitView;

    use super::worktree_label;

    #[test]
    fn the_main_checkout_is_named_apart_from_its_worktrees() {
        let view = |branch: Option<&str>, main| GitView {
            repo: None,
            branch: branch.map(str::to_owned),
            main,
            base: None,
            changes: std::sync::Arc::default(),
            branch_diff: std::sync::Arc::default(),
            error: None,
        };
        let cases = [
            ("/code/api", Some(view(Some("fix"), true)), "main · fix"),
            ("/code/api", Some(view(None, true)), "main · detached"),
            ("/code/api/.wt/fix", Some(view(Some("fix"), false)), "fix"),
            ("/code/api/.wt/fix", None, "fix"),
        ];
        for (folder, git, expected) in cases {
            assert_eq!(
                worktree_label(Path::new(folder), git.as_ref()),
                expected,
                "{folder}"
            );
        }
    }
}
