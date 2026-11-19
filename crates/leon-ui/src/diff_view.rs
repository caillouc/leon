//! The right pane's Changes and Branch tabs (GIT-1, GIT-3, GIT-4): what
//! changed, file by file. Clicking a file opens the drawer with every diff,
//! at that file.

use std::sync::Arc;

use gpui::{
    AnyElement, Context, Focusable, FontWeight, IntoElement, SharedString, Window, div, prelude::*,
};
use leon_domain::{
    FileDiff, FlowId, FoldPattern, LineKind, ReviewComment, fold_reason, folded_last,
};

use crate::diff_rows::{counts, path_text, status_letter};
use crate::right::Pending;
use crate::theme;
use crate::widgets::empty;
use crate::workspace::{Workspace, text_of};

/// Which diff of a flow.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum DiffSource {
    /// Uncommitted changes against `HEAD` (GIT-1, GIT-2).
    Changes,
    /// The branch against its base (GIT-3).
    Branch,
    /// The pull request shown, as GitHub's "Files changed" shows it,
    /// read-only (GH-3).
    PullRequest,
}

/// The lines a comment is being written on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DraftAt {
    pub flow: FlowId,
    pub source: DiffSource,
    pub path: String,
    /// First and last line numbers (equal for one line).
    pub line: u32,
    pub end: u32,
    /// Kind of the last line, under which the comment box shows.
    pub kind: LineKind,
    /// The lines chosen, as (hunk, line) in the file's diff: they stay
    /// highlighted while the comment is written.
    pub lines: ((usize, usize), (usize, usize)),
    /// The lines, one per line.
    pub code: String,
}

/// A diff's files, and what they are compared with.
pub(crate) struct DiffFiles {
    pub files: Arc<Vec<FileDiff>>,
    /// "vs HEAD", "since main".
    pub against: String,
}

impl Workspace {
    /// The files of a diff, or why there is nothing to show.
    pub(crate) fn diff_files(&self, id: FlowId, source: DiffSource) -> Result<DiffFiles, String> {
        if source == DiffSource::PullRequest {
            return self.pr_diff_files();
        }
        if self.folder(id).is_none() {
            return Err("No worktree yet: one shows once an agent works in it.".to_owned());
        }
        let Some(git) = self.folder_git(id) else {
            return Err("Reading git…".to_owned());
        };
        if let Some(error) = &git.error {
            return Err(format!("Git could not be read: {error}"));
        }
        let (files, against) = match source {
            DiffSource::Changes => (Arc::clone(&git.changes), "vs HEAD".to_owned()),
            DiffSource::Branch | DiffSource::PullRequest => (
                Arc::clone(&git.branch_diff),
                git.base.as_ref().map_or_default(|b| format!("since {b}")),
            ),
        };
        // Folded files come last (GIT-4), in the list and the drawer alike.
        let files = folded_last(&files, &self.snapshot.fold);
        if files.is_empty() {
            return Err(match (source, &git.base) {
                (DiffSource::Changes, _) => {
                    "No uncommitted changes. The working tree is clean.".to_owned()
                }
                (DiffSource::Branch | DiffSource::PullRequest, Some(base)) => format!(
                    "No commits on this branch yet compared with {base}. Uncommitted work shows in Changes."
                ),
                (DiffSource::Branch | DiffSource::PullRequest, None) => {
                    "No base branch found to compare with.".to_owned()
                }
            });
        }
        Ok(DiffFiles { files, against })
    }

    /// The pull request's files as GitHub gave them, or why there are none
    /// yet (GH-3).
    fn pr_diff_files(&self) -> Result<DiffFiles, String> {
        let diff = self
            .snapshot
            .pr_diff
            .as_ref()
            .ok_or_else(|| "No pull request.".to_owned())?;
        let files = match &diff.files {
            None => return Err("Reading its changes from GitHub…".to_owned()),
            Some(Err(error)) => return Err(format!("GitHub could not give its changes: {error}")),
            Some(Ok(files)) => folded_last(files, &self.snapshot.fold),
        };
        if files.is_empty() {
            return Err("This pull request changes no file.".to_owned());
        }
        Ok(DiffFiles {
            files,
            against: format!("as GitHub shows {}", diff.pr.short()),
        })
    }

    /// The tab: totals, the fold rule, and one row per file.
    pub(crate) fn render_diff(
        &mut self,
        id: FlowId,
        source: DiffSource,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let diff = match self.diff_files(id, source) {
            Ok(diff) => diff,
            Err(text) => return empty(text),
        };
        let fold = Arc::clone(&self.snapshot.fold);
        let shown = self.folder(id);
        let review: Vec<&Pending> = self
            .right
            .review
            .get(&id)
            .map_or_default(|r| r.iter().filter(|p| p.folder == shown).collect());
        let added: u32 = diff.files.iter().map(|f| f.added).sum();
        let removed: u32 = diff.files.iter().map(|f| f.removed).sum();
        let n = diff.files.len();
        let rows = diff.files.iter().enumerate().map(|(ix, file)| {
            let folded = fold_reason(&fold, &file.path).is_some();
            let comments = review
                .iter()
                .filter(|p| p.source == source && p.comment.path == file.path)
                .count();
            let path = file.path.clone();
            div()
                .id(("diff-index", ix))
                .flex()
                .items_center()
                .gap_2()
                .px_3()
                .py(crate::theme::rem(7.))
                .when(ix > 0, |d| d.border_t_1().border_color(theme::border()))
                .cursor_pointer()
                .hover(|s| s.bg(theme::surface_2()))
                .child(status_letter(file.status))
                .child(path_text(&file.path).when(folded, |d| d.text_color(theme::fg_muted())))
                .when(folded, |d| d.child(tag("folded".to_owned())))
                .when(comments > 0, |d| {
                    d.child(
                        tag(format!(
                            "{comments} comment{}",
                            if comments == 1 { "" } else { "s" }
                        ))
                        .text_color(theme::warn()),
                    )
                })
                .child(counts(file.added, file.removed))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.open_drawer(source, path.clone(), cx);
                }))
        });
        div()
            .id(SharedString::from(format!("diff-files-{source:?}")))
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .p_3()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .text_size(crate::theme::rem(12.5))
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(format!("{n} file{}", if n == 1 { "" } else { "s" })),
                    )
                    .child(counts(added, removed))
                    .child(div().text_color(theme::fg_muted()).child(diff.against)),
            )
            // `flex_none`: in a scrolling column a child that may shrink is
            // clipped instead of scrolled.
            .child(
                div()
                    .flex_none()
                    .flex()
                    .flex_col()
                    .rounded_lg()
                    .border_1()
                    .border_color(theme::border())
                    .bg(theme::window())
                    .overflow_hidden()
                    .children(rows),
            )
            .when(!fold.is_empty(), |d| {
                d.child(
                    div()
                        .text_size(crate::theme::rem(11.5))
                        .text_color(theme::fg_faint())
                        .child(format!("Folded by {}: change it in Settings.", fold_rule(&fold))),
                )
            })
            .child(
                div()
                    .text_size(crate::theme::rem(11.5))
                    .text_color(theme::fg_faint())
                    .child("Click a file to read the diffs. Hover a line there and click + to comment."),
            )
            .into_any_element()
    }

    pub(crate) fn start_comment(
        &mut self,
        at: DraftAt,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.right.draft = Some(at);
        self.right
            .comment
            .update(cx, |state, cx| state.set_value("", window, cx));
        self.right.comment.focus_handle(cx).focus(window, cx);
        cx.notify();
    }

    /// Adds the comment being written to the pending review (GIT-5).
    pub(crate) fn save_comment(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = text_of(&self.right.comment, cx);
        let Some(at) = self.right.draft.take() else {
            return;
        };
        if !text.trim().is_empty() {
            let folder = self.folder(at.flow);
            self.right.review.entry(at.flow).or_default().push(Pending {
                source: at.source,
                folder,
                comment: ReviewComment {
                    path: at.path,
                    line: at.line,
                    end: at.end,
                    kind: at.kind,
                    code: at.code,
                    text: text.trim().to_owned(),
                },
            });
        }
        self.right
            .comment
            .update(cx, |state, cx| state.set_value("", window, cx));
        cx.notify();
    }
}

fn fold_rule(fold: &[FoldPattern]) -> String {
    if fold.is_empty() {
        return String::new();
    }
    let patterns: Vec<&str> = fold.iter().map(FoldPattern::source).collect();
    patterns.join(", ")
}

/// A small grey label next to a file.
fn tag(text: String) -> gpui::Div {
    div()
        .flex_none()
        .px(crate::theme::rem(6.))
        .rounded(crate::theme::rem(4.))
        .bg(theme::surface_2())
        .text_size(crate::theme::rem(10.5))
        .text_color(theme::fg_muted())
        .child(text)
}
