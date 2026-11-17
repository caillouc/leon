//! The diff drawer's rows (GIT-2, GIT-3, GIT-5), virtualised: each file's
//! header, and when the file is open its hunks and lines, with the pending
//! comments and the comment being written.

use std::rc::Rc;
use std::sync::Arc;

use gpui::{
    AnyElement, App, Context, Entity, FontWeight, IntoElement, ListAlignment, ListOffset,
    ListState, MouseButton, MouseMoveEvent, SharedString, Styled, WeakEntity, div, list,
    prelude::*, px,
};
use gpui_component::input::{Textarea, TextareaState};
use leon_domain::{ConversationId, DiffLine, FileDiff, FileStatus, LineKind, fold_reason};

use crate::diff_view::{DiffSource, DraftAt};
use crate::icons::chevron;
use crate::right::Pending;
use crate::theme;
use crate::widgets::{button, empty, primary};
use crate::workspace::Workspace;

/// One row of the virtualised diff.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Row {
    File(usize),
    Hunk(usize, usize),
    Line(usize, usize, usize),
    /// A pending comment, by index in the conversation's review.
    Comment(usize),
    Draft,
}

/// Lines chosen by dragging a line's + (GIT-5): the file, and the first
/// and current line, each as (hunk, line) in the file's diff.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Selecting {
    pub conversation: ConversationId,
    pub source: DiffSource,
    pub path: String,
    pub from: (usize, usize),
    pub to: (usize, usize),
}

impl Selecting {
    /// Whether line `l` of hunk `h` is in the selection.
    fn holds(&self, h: usize, l: usize) -> bool {
        let (a, b) = if self.from <= self.to {
            (self.from, self.to)
        } else {
            (self.to, self.from)
        };
        (a..=b).contains(&(h, l))
    }
}

/// The comment place for lines `from`..=`to` of `file` (any order).
fn draft_for(file: &FileDiff, selecting: &Selecting) -> Option<DraftAt> {
    let lines: Vec<&DiffLine> = file
        .hunks
        .iter()
        .enumerate()
        .flat_map(|(h, hunk)| {
            hunk.lines
                .iter()
                .enumerate()
                .map(move |(l, line)| ((h, l), line))
        })
        .filter(|((h, l), _)| selecting.holds(*h, *l))
        .map(|(_, line)| line)
        .collect();
    let (first, last) = (lines.first()?, lines.last()?);
    Some(DraftAt {
        conversation: selecting.conversation,
        source: selecting.source,
        path: file.path.clone(),
        line: line_number(first),
        end: line_number(last).max(line_number(first)),
        kind: last.kind,
        code: lines
            .iter()
            .map(|l| l.text.as_str())
            .collect::<Vec<_>>()
            .join("\n"),
    })
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
    review: Vec<Pending>,
    comment: Entity<TextareaState>,
    /// Lines being chosen, when in this diff.
    selecting: Option<Selecting>,
    view: WeakEntity<Workspace>,
}

impl Workspace {
    /// Every file of a diff in one list, files matching `diff.fold` folded;
    /// scrolled to the file `scroll_to` once, when given.
    pub(crate) fn diff_rows(
        &mut self,
        id: ConversationId,
        source: DiffSource,
        scroll_to: Option<&str>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let files = match self.diff_files(id, source) {
            Ok(diff) => diff.files,
            Err(text) => return empty(text),
        };
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
        let (state, rows) = self.diff_list((id, source), rows);
        let target = scroll_to.and_then(|path| {
            let f = files.iter().position(|file| file.path == path)?;
            rows.iter().position(|r| *r == Row::File(f))
        });
        if let Some(item_ix) = target {
            state.scroll_to(ListOffset {
                item_ix,
                offset_in_item: px(0.),
            });
        }
        let data = Rc::new(Data {
            conversation: id,
            source,
            files,
            rows: Arc::clone(&rows),
            opened,
            folded_by,
            review,
            comment: self.right.comment.clone(),
            selecting: self
                .right
                .selecting
                .clone()
                .filter(|s| s.conversation == id && s.source == source),
            view: cx.entity().downgrade(),
        });
        let rows = list(state, move |ix, _window, cx| {
            let row = match data.rows.get(ix) {
                Some(row) => render_row(&data, *row, cx),
                None => div().into_any_element(),
            };
            div().px_4().child(row).into_any_element()
        })
        .flex_1()
        .min_h_0()
        .py_2();
        // The drawer hides the window behind it from the mouse, so the end
        // of a drag over lines is caught here.
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| this.finish_selection(window, cx)),
            )
            .child(rows)
            .into_any_element()
    }

    /// The list state of a diff, reset (keeping the scroll position) when its
    /// rows change.
    fn diff_list(
        &mut self,
        key: (ConversationId, DiffSource),
        rows: Vec<Row>,
    ) -> (ListState, Arc<Vec<Row>>) {
        let list = self.right.lists.entry(key).or_insert_with(|| DiffList {
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

    fn toggle_file(&mut self, id: ConversationId, source: DiffSource, path: String, open: bool) {
        self.right.open.insert((id, source, path), !open);
    }

    /// The mouse went up: the chosen lines get the comment box.
    pub(crate) fn finish_selection(&mut self, window: &mut gpui::Window, cx: &mut Context<Self>) {
        let Some(selecting) = self.right.selecting.take() else {
            return;
        };
        let draft = self
            .diff_files(selecting.conversation, selecting.source)
            .ok()
            .and_then(|diff| {
                let file = diff.files.iter().find(|f| f.path == selecting.path)?;
                draft_for(file, &selecting)
            });
        match draft {
            Some(at) => self.start_comment(at, window, cx),
            None => cx.notify(),
        }
    }
}

/// Rows of a diff: each file's header, and when the file is open its hunks
/// and lines, each line followed by its pending comments and the draft.
fn build_rows(
    files: &[FileDiff],
    opened: &[bool],
    source: DiffSource,
    review: &[Pending],
    draft: Option<&DraftAt>,
) -> Vec<Row> {
    let mut rows = Vec::new();
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
                // A comment shows under the last line it is about.
                rows.extend(review.iter().enumerate().filter_map(|(ix, p)| {
                    (p.source == source && on_line(&p.comment.path, p.comment.end, p.comment.kind))
                        .then_some(Row::Comment(ix))
                }));
                if draft.is_some_and(|d| on_line(&d.path, d.end, d.kind)) {
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

fn render_row(data: &Rc<Data>, row: Row, cx: &mut App) -> AnyElement {
    match row {
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
        Row::Line(f, h, l) => line_row(data, f, h, l),
        Row::Comment(ix) => pending_comment(data, ix),
        Row::Draft => draft_box(data, cx),
    }
}

fn mono(d: gpui::Div) -> gpui::Div {
    d.font_family(theme::MONO).text_size(px(11.5))
}

pub(crate) fn status_letter(status: FileStatus) -> AnyElement {
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

pub(crate) fn counts(added: u32, removed: u32) -> AnyElement {
    mono(div().flex().flex_none().gap(px(6.)))
        .child(div().text_color(theme::ok()).child(format!("+{added}")))
        .child(div().text_color(theme::err()).child(format!("−{removed}")))
        .into_any_element()
}

pub(crate) fn path_text(path: &str) -> gpui::Div {
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
        .child(chevron(open, theme::fg_muted(), 14.))
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

fn line_row(data: &Rc<Data>, f: usize, h: usize, l: usize) -> AnyElement {
    let line = &data.files[f].hunks[h].lines[l];
    let path = data.files[f].path.clone();
    let chosen = data
        .selecting
        .as_ref()
        .is_some_and(|s| s.path == path && s.holds(h, l));
    let (bg, gutter, sign) = match line.kind {
        LineKind::Added => (Some(theme::add_bg()), Some(theme::add_gutter()), "+"),
        LineKind::Removed => (Some(theme::del_bg()), Some(theme::del_gutter()), "-"),
        LineKind::Context => (None, None, " "),
    };
    let bg = if chosen {
        Some(theme::accent_soft())
    } else {
        bg
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
    let start = Selecting {
        conversation: data.conversation,
        source: data.source,
        path: path.clone(),
        from: (h, l),
        to: (h, l),
    };
    let (down_view, move_view) = (data.view.clone(), data.view.clone());
    let group: SharedString = "diff-line".into();
    mono(div().flex().group(group.clone()).when_some(bg, Styled::bg))
        // While choosing lines, the line under the mouse ends the range.
        .on_mouse_move(move |e: &MouseMoveEvent, _, cx| {
            if e.pressed_button != Some(MouseButton::Left) {
                return;
            }
            let path = path.clone();
            move_view
                .update(cx, |this, cx| {
                    if let Some(s) = this.right.selecting.as_mut()
                        && s.path == path
                        && s.to != (h, l)
                    {
                        s.to = (h, l);
                        cx.notify();
                    }
                })
                .ok();
        })
        .child(number(line.old))
        .child(number(line.new))
        .child(
            div()
                .id(SharedString::from(format!(
                    "comment-{}-{h}-{l}",
                    start.path
                )))
                .w(px(16.))
                .flex_none()
                .flex()
                .justify_center()
                .text_color(theme::on_accent())
                .opacity(if chosen { 1. } else { 0. })
                .group_hover(group, |s| s.opacity(1.))
                .cursor_pointer()
                .child(
                    div()
                        .px(px(3.))
                        .rounded(px(3.))
                        .bg(theme::accent())
                        .child("+"),
                )
                // Press on + and drag over more lines; the comment box opens
                // when the mouse goes up.
                .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                    cx.stop_propagation();
                    let start = start.clone();
                    down_view
                        .update(cx, |this, cx| {
                            this.right.selecting = Some(start);
                            cx.notify();
                        })
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

    use super::{Row, build_rows};
    use crate::diff_view::{DiffSource, DraftAt};
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
                end: line,
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
            end: 2,
            kind: LineKind::Added,
            code: "x".into(),
        };
        let review = [
            pending(DiffSource::Changes, LineKind::Removed, 2),
            pending(DiffSource::Branch, LineKind::Context, 1),
        ];
        let cases = [
            ([false, false], None, vec![Row::File(0), Row::File(1)]),
            (
                [true, false],
                Some(&draft),
                vec![
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

    #[test]
    fn dragged_lines_make_one_comment_place() {
        let selecting = super::Selecting {
            conversation: leon_domain::ConversationId(1),
            source: DiffSource::Changes,
            path: "a.rs".into(),
            // Dragged upwards: from the last line to the first.
            from: (0, 2),
            to: (0, 0),
        };
        let at = super::draft_for(&file("a.rs"), &selecting).unwrap();
        assert_eq!(
            (at.line, at.end, at.kind, at.code.as_str()),
            (1, 2, LineKind::Added, "x\nx\nx")
        );
    }
}
