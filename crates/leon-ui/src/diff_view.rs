//! A diff in the right pane (GIT-1..5), virtualised: summary and file list,
//! then each file, folded by hand or by `diff.fold`, with comments on lines.

use std::rc::Rc;
use std::sync::Arc;

use gpui::{
    AnyElement, App, Context, Entity, Focusable, FontWeight, IntoElement, ListAlignment,
    ListOffset, ListState, SharedString, Styled, WeakEntity, Window, div, list, prelude::*, px,
};
use gpui_component::input::{Textarea, TextareaState};
use leon_domain::{
    ConversationId, DiffLine, FileDiff, FileStatus, FoldPattern, LineKind, ReviewComment,
    fold_reason,
};

use crate::right::Pending;
use crate::theme;
use crate::widgets::{button, empty, primary};
use crate::workspace::{Workspace, text_of};

/// Which diff of a conversation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum DiffSource {
    /// Uncommitted changes against `HEAD` (GIT-1, GIT-2).
    Changes,
    /// The branch against its base (GIT-3).
    Branch,
}

/// The line a comment is being written on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DraftAt {
    pub conversation: ConversationId,
    pub source: DiffSource,
    pub path: String,
    pub line: u32,
    pub kind: LineKind,
    pub code: String,
}

/// One row of the virtualised diff.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Row {
    Summary,
    File(usize),
    Hunk(usize, usize),
    Line(usize, usize, usize),
    /// A pending comment, by index in the conversation's review.
    Comment(usize),
    Draft,
}

/// Scroll state of one diff and the rows it was laid out for.
pub(crate) struct DiffList {
    state: ListState,
    rows: Arc<Vec<Row>>,
}

/// Everything a row needs to render, shared by the list's rows.
struct Data {
    conversation: ConversationId,
    source: DiffSource,
    files: Arc<Vec<FileDiff>>,
    rows: Arc<Vec<Row>>,
    opened: Vec<bool>,
    folded_by: Vec<Option<String>>,
    fold_rule: String,
    subtitle: String,
    review: Vec<Pending>,
    comment: Entity<TextareaState>,
    state: ListState,
    view: WeakEntity<Workspace>,
}

impl Workspace {
    pub(crate) fn render_diff(
        &mut self,
        id: ConversationId,
        source: DiffSource,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(git) = self.snapshot.git.get(&id).cloned() else {
            return empty("Reading git…");
        };
        if let Some(error) = &git.error {
            return empty(format!("Git could not be read: {error}"));
        }
        let (files, subtitle) = match source {
            DiffSource::Changes => (Arc::clone(&git.changes), "vs HEAD".to_owned()),
            DiffSource::Branch => (
                Arc::clone(&git.branch_diff),
                git.base.as_ref().map_or_default(|b| format!("since {b}")),
            ),
        };
        if files.is_empty() {
            return empty(match (source, &git.base) {
                (DiffSource::Changes, _) => {
                    "No uncommitted changes. The working tree is clean.".to_owned()
                }
                (DiffSource::Branch, Some(base)) => format!(
                    "No commits on this branch yet compared with {base}. Uncommitted work shows in Changes."
                ),
                (DiffSource::Branch, None) => "No base branch found to compare with.".to_owned(),
            });
        }
        let fold = Arc::clone(&self.snapshot.fold);
        let folded_by: Vec<Option<String>> = files
            .iter()
            .map(|f| fold_reason(&fold, &f.path).map(|p| p.source().to_owned()))
            .collect();
        let opened: Vec<bool> = files
            .iter()
            .zip(&folded_by)
            .map(|(f, by)| {
                self.right
                    .open
                    .get(&(id, source, f.path.clone()))
                    .copied()
                    .unwrap_or(by.is_none())
            })
            .collect();
        let review = self.right.review.get(&id).cloned().unwrap_or_default();
        let draft = self
            .right
            .draft
            .clone()
            .filter(|d| d.conversation == id && d.source == source);
        let rows = build_rows(&files, &opened, source, &review, draft.as_ref());
        let (state, rows) = self.diff_list(id, source, rows);
        let data = Rc::new(Data {
            conversation: id,
            source,
            files,
            rows: Arc::clone(&rows),
            opened,
            folded_by,
            fold_rule: fold_rule(&fold),
            subtitle,
            review,
            comment: self.right.comment.clone(),
            state: state.clone(),
            view: cx.entity().downgrade(),
        });
        list(state, move |ix, _window, cx| {
            let row = match data.rows.get(ix) {
                Some(row) => render_row(&data, *row, cx),
                None => div().into_any_element(),
            };
            div().px_3().child(row).into_any_element()
        })
        .flex_1()
        .min_h_0()
        .py_2()
        .into_any_element()
    }

    /// The list state of a diff, reset (keeping the scroll position) when its
    /// rows change.
    fn diff_list(
        &mut self,
        id: ConversationId,
        source: DiffSource,
        rows: Vec<Row>,
    ) -> (ListState, Arc<Vec<Row>>) {
        let list = self
            .right
            .lists
            .entry((id, source))
            .or_insert_with(|| DiffList {
                state: ListState::new(0, ListAlignment::Top, px(200.)),
                rows: Arc::new(Vec::new()),
            });
        if *list.rows != rows {
            let top = list.state.logical_scroll_top();
            list.state.reset(rows.len());
            list.state.scroll_to(top);
            list.rows = Arc::new(rows);
        }
        (list.state.clone(), Arc::clone(&list.rows))
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
            self.right
                .review
                .entry(at.conversation)
                .or_default()
                .push(Pending {
                    source: at.source,
                    comment: ReviewComment {
                        path: at.path,
                        line: at.line,
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

    fn toggle_file(&mut self, id: ConversationId, source: DiffSource, path: String, open: bool) {
        self.right.open.insert((id, source, path), !open);
    }
}

/// Rows of a diff: the summary, each file, and when a file is open its hunks
/// and lines, each line followed by its pending comments and the draft.
fn build_rows(
    files: &[FileDiff],
    opened: &[bool],
    source: DiffSource,
    review: &[Pending],
    draft: Option<&DraftAt>,
) -> Vec<Row> {
    let mut rows = vec![Row::Summary];
    for (f, file) in files.iter().enumerate() {
        rows.push(Row::File(f));
        if !opened.get(f).copied().unwrap_or(true) {
            continue;
        }
        for (h, hunk) in file.hunks.iter().enumerate() {
            rows.push(Row::Hunk(f, h));
            for (l, line) in hunk.lines.iter().enumerate() {
                rows.push(Row::Line(f, h, l));
                let at = line_number(line);
                let on_line = |path: &str, n: u32, kind: LineKind| {
                    path == file.path && n == at && kind == line.kind
                };
                rows.extend(review.iter().enumerate().filter_map(|(ix, p)| {
                    (p.source == source && on_line(&p.comment.path, p.comment.line, p.comment.kind))
                        .then_some(Row::Comment(ix))
                }));
                if draft.is_some_and(|d| on_line(&d.path, d.line, d.kind)) {
                    rows.push(Row::Draft);
                }
            }
        }
    }
    rows
}

/// The number a comment refers to: the old file's for a removed line, the
/// new file's otherwise.
fn line_number(line: &DiffLine) -> u32 {
    match line.kind {
        LineKind::Removed => line.old,
        LineKind::Added | LineKind::Context => line.new,
    }
    .unwrap_or(0)
}

fn fold_rule(fold: &[FoldPattern]) -> String {
    if fold.is_empty() {
        return String::new();
    }
    let patterns: Vec<&str> = fold.iter().map(FoldPattern::source).collect();
    format!("diff.fold = [{}]", patterns.join(", "))
}

fn render_row(data: &Rc<Data>, row: Row, cx: &mut App) -> AnyElement {
    match row {
        Row::Summary => summary(data),
        Row::File(f) => file_header(data, f),
        Row::Hunk(f, h) => {
            let header = data.files[f].hunks[h].header.clone();
            mono(
                div()
                    .px_2()
                    .py(px(2.))
                    .bg(theme::accent_soft())
                    .text_color(theme::fg_muted()),
            )
            .child(header)
            .into_any_element()
        }
        Row::Line(f, h, l) => line_row(data, f, &data.files[f].hunks[h].lines[l]),
        Row::Comment(ix) => pending_comment(data, ix),
        Row::Draft => draft_box(data, cx),
    }
}

fn mono(d: gpui::Div) -> gpui::Div {
    d.font_family(theme::MONO).text_size(px(11.5))
}

fn status_letter(status: FileStatus) -> AnyElement {
    let (letter, color) = match status {
        FileStatus::Added | FileStatus::Untracked => ("A", theme::ok()),
        FileStatus::Modified => ("M", theme::warn()),
        FileStatus::Deleted => ("D", theme::err()),
        FileStatus::Renamed => ("R", theme::accent()),
    };
    mono(
        div()
            .w(px(12.))
            .flex_none()
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(color),
    )
    .child(letter)
    .into_any_element()
}

fn counts(added: u32, removed: u32) -> AnyElement {
    mono(div().flex().flex_none().gap(px(6.)))
        .child(div().text_color(theme::ok()).child(format!("+{added}")))
        .child(div().text_color(theme::err()).child(format!("−{removed}")))
        .into_any_element()
}

fn summary(data: &Rc<Data>) -> AnyElement {
    let added: u32 = data.files.iter().map(|f| f.added).sum();
    let removed: u32 = data.files.iter().map(|f| f.removed).sum();
    let n = data.files.len();
    let file_rows = data.files.iter().enumerate().map(|(f, file)| {
        let target = data
            .rows
            .iter()
            .position(|r| *r == Row::File(f))
            .unwrap_or(0);
        let state = data.state.clone();
        div()
            .id(("diff-index", f))
            .flex()
            .items_center()
            .gap_2()
            .px_2()
            .py(px(4.))
            .border_t_1()
            .border_color(theme::border())
            .cursor_pointer()
            .hover(|s| s.bg(theme::surface_2()))
            .child(status_letter(file.status))
            .child(path_text(&file.path))
            .child(counts(file.added, file.removed))
            .on_click(move |_, _, _| {
                state.scroll_to(ListOffset {
                    item_ix: target,
                    offset_in_item: px(0.),
                });
            })
    });
    div()
        .flex()
        .flex_col()
        .gap_2()
        .pb_2()
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .text_size(px(12.5))
                .child(
                    div()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(format!("{n} file{}", if n == 1 { "" } else { "s" })),
                )
                .child(counts(added, removed))
                .child(
                    div()
                        .text_color(theme::fg_muted())
                        .child(data.subtitle.clone()),
                ),
        )
        .when(!data.fold_rule.is_empty(), |d| {
            d.child(
                mono(div().text_size(px(11.)).text_color(theme::fg_faint()))
                    .child(data.fold_rule.clone()),
            )
        })
        .child(
            div()
                .flex()
                .flex_col()
                .rounded_lg()
                .border_1()
                .border_color(theme::border())
                .bg(theme::window())
                .overflow_hidden()
                .children(file_rows),
        )
        .child(
            div()
                .text_size(px(11.5))
                .text_color(theme::fg_faint())
                .child("Hover a line and click + to comment. Comments pile up into one review."),
        )
        .into_any_element()
}

fn path_text(path: &str) -> gpui::Div {
    mono(
        div()
            .flex_1()
            .min_w_0()
            .overflow_hidden()
            .whitespace_nowrap()
            .text_ellipsis(),
    )
    .child(path.to_owned())
}

fn file_header(data: &Rc<Data>, f: usize) -> AnyElement {
    let file = &data.files[f];
    let open = data.opened[f];
    let (id, source, path, view) = (
        data.conversation,
        data.source,
        file.path.clone(),
        data.view.clone(),
    );
    let tag = |text: String| {
        div()
            .flex_none()
            .px(px(6.))
            .rounded(px(4.))
            .bg(theme::surface_2())
            .text_size(px(10.5))
            .text_color(theme::fg_muted())
            .child(text)
    };
    div()
        .id(("diff-file", f))
        .mt_2()
        .flex()
        .items_center()
        .gap(px(7.))
        .px_2()
        .py(px(6.))
        .rounded_md()
        .border_1()
        .border_color(theme::border())
        .bg(theme::surface())
        .text_size(px(12.))
        .cursor_pointer()
        .hover(|s| s.bg(theme::surface_2()))
        .child(
            div()
                .text_color(theme::fg_faint())
                .child(if open { "▾" } else { "▸" }),
        )
        .child(status_letter(file.status))
        .child(path_text(&file.path))
        .children(
            data.folded_by[f]
                .as_ref()
                .map(|by| tag(format!("auto-folded by {by}"))),
        )
        .children(file.binary.then(|| tag("binary".to_owned())))
        .child(counts(file.added, file.removed))
        .on_click(move |_, _, cx| {
            let path = path.clone();
            view.update(cx, |this, cx| {
                this.toggle_file(id, source, path, open);
                cx.notify();
            })
            .ok();
        })
        .into_any_element()
}

fn line_row(data: &Rc<Data>, f: usize, line: &DiffLine) -> AnyElement {
    let (bg, gutter, sign) = match line.kind {
        LineKind::Added => (Some(theme::add_bg()), Some(theme::add_gutter()), "+"),
        LineKind::Removed => (Some(theme::del_bg()), Some(theme::del_gutter()), "-"),
        LineKind::Context => (None, None, " "),
    };
    let number = |n: Option<u32>| {
        div()
            .w(px(34.))
            .flex_none()
            .pr(px(6.))
            .flex()
            .justify_end()
            .text_color(theme::fg_faint())
            .when_some(gutter, Styled::bg)
            .child(n.map_or_default(|n| n.to_string()))
    };
    let at = DraftAt {
        conversation: data.conversation,
        source: data.source,
        path: data.files[f].path.clone(),
        line: line_number(line),
        kind: line.kind,
        code: line.text.clone(),
    };
    let view = data.view.clone();
    let group: SharedString = "diff-line".into();
    mono(div().flex().group(group.clone()).when_some(bg, Styled::bg))
        .child(number(line.old))
        .child(number(line.new))
        .child(
            div()
                .id(SharedString::from(format!(
                    "comment-{}-{}-{:?}",
                    at.path, at.line, at.kind
                )))
                .w(px(16.))
                .flex_none()
                .flex()
                .justify_center()
                .text_color(theme::on_accent())
                .opacity(0.)
                .group_hover(group, |s| s.opacity(1.))
                .cursor_pointer()
                .child(
                    div()
                        .px(px(3.))
                        .rounded(px(3.))
                        .bg(theme::accent())
                        .child("+"),
                )
                .on_click(move |_, window, cx| {
                    let at = at.clone();
                    view.update(cx, |this, cx| this.start_comment(at, window, cx))
                        .ok();
                }),
        )
        .child(
            div()
                .w(px(12.))
                .flex_none()
                .text_color(theme::fg_faint())
                .child(sign),
        )
        .child(div().flex_1().min_w_0().pr_2().child(line.text.clone()))
        .into_any_element()
}

fn pending_comment(data: &Rc<Data>, ix: usize) -> AnyElement {
    let Some(pending) = data.review.get(ix) else {
        return div().into_any_element();
    };
    let (id, view) = (data.conversation, data.view.clone());
    div()
        .my_1()
        .ml(px(84.))
        .pl_2()
        .border_l_2()
        .border_color(theme::warn())
        .flex()
        .gap_2()
        .text_size(px(12.))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .child(
                    div()
                        .text_size(px(11.))
                        .text_color(theme::warn())
                        .child("Pending comment"),
                )
                .child(pending.comment.text.clone()),
        )
        .child(
            div()
                .id(("comment-remove", ix))
                .px_1()
                .text_color(theme::fg_faint())
                .cursor_pointer()
                .hover(|s| s.text_color(theme::fg()))
                .child("×")
                .on_click(move |_, _, cx| {
                    view.update(cx, |this, cx| {
                        if let Some(review) = this.right.review.get_mut(&id)
                            && ix < review.len()
                        {
                            review.remove(ix);
                        }
                        cx.notify();
                    })
                    .ok();
                }),
        )
        .into_any_element()
}

fn draft_box(data: &Rc<Data>, _cx: &mut App) -> AnyElement {
    let (cancel_view, save_view) = (data.view.clone(), data.view.clone());
    div()
        .my_1()
        .ml(px(84.))
        .max_w(px(340.))
        .p_2()
        .flex()
        .flex_col()
        .gap(px(6.))
        .rounded_md()
        .border_1()
        .border_color(theme::border_strong())
        .bg(theme::surface())
        .child(Textarea::new(&data.comment))
        .child(
            div()
                .flex()
                .justify_end()
                .gap(px(6.))
                .child(button("comment-cancel", "Cancel", move |_, _, cx| {
                    cancel_view
                        .update(cx, |this, cx| {
                            this.right.draft = None;
                            cx.notify();
                        })
                        .ok();
                }))
                .child(primary(
                    "comment-save",
                    "Add to review",
                    move |_, window, cx| {
                        save_view
                            .update(cx, |this, cx| this.save_comment(window, cx))
                            .ok();
                    },
                )),
        )
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use leon_domain::{DiffLine, FileDiff, FileStatus, Hunk, LineKind, ReviewComment};

    use super::{DiffSource, DraftAt, Row, build_rows};
    use crate::right::Pending;

    fn file(path: &str) -> FileDiff {
        let line = |kind, old, new| DiffLine {
            kind,
            old,
            new,
            text: "x".into(),
        };
        FileDiff {
            path: path.into(),
            status: FileStatus::Modified,
            added: 1,
            removed: 1,
            binary: false,
            hunks: vec![Hunk {
                header: "@@ -1,2 +1,2 @@".into(),
                lines: vec![
                    line(LineKind::Context, Some(1), Some(1)),
                    line(LineKind::Removed, Some(2), None),
                    line(LineKind::Added, None, Some(2)),
                ],
            }],
        }
    }

    fn pending(source: DiffSource, kind: LineKind, line: u32) -> Pending {
        Pending {
            source,
            comment: ReviewComment {
                path: "a.rs".into(),
                line,
                kind,
                code: "x".into(),
                text: "why".into(),
            },
        }
    }

    #[test]
    fn rows_follow_folds_comments_and_the_draft() {
        let files = [file("a.rs"), file("Cargo.lock")];
        let draft = DraftAt {
            conversation: leon_domain::ConversationId(1),
            source: DiffSource::Changes,
            path: "a.rs".into(),
            line: 2,
            kind: LineKind::Added,
            code: "x".into(),
        };
        let review = [
            pending(DiffSource::Changes, LineKind::Removed, 2),
            pending(DiffSource::Branch, LineKind::Context, 1),
        ];
        let cases = [
            (
                [false, false],
                None,
                vec![Row::Summary, Row::File(0), Row::File(1)],
            ),
            (
                [true, false],
                Some(&draft),
                vec![
                    Row::Summary,
                    Row::File(0),
                    Row::Hunk(0, 0),
                    Row::Line(0, 0, 0),
                    Row::Line(0, 0, 1),
                    Row::Comment(0),
                    Row::Line(0, 0, 2),
                    Row::Draft,
                    Row::File(1),
                ],
            ),
        ];
        for (opened, draft, expected) in cases {
            assert_eq!(
                build_rows(&files, &opened, DiffSource::Changes, &review, draft),
                expected
            );
        }
    }
}
