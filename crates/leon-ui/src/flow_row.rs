//! A flow's row in the left pane (NAV-4, NAV-5, NAV-6): what it asks
//! of the user at a glance — working, needs you, to review, failed — and its
//! pull requests.

use gpui::{AnyElement, Context, Div, IntoElement, Rgba, SharedString, div, prelude::*};
use gpui_component::input::Input;
use leon_core::FlowView;
use leon_domain::{Attention, FlowId};

use crate::icons::{Icon, icon};
use crate::row_menu::MenuFor;
use crate::spinner::spinner;
use crate::theme;
use crate::workspace::{Screen, Workspace};

/// Whether a click on `row` opens it while `renaming` has its title in an
/// input (NAV-8): a click in the title being edited places the caret or
/// selects text, and Enter still saves; a click on another row opens it,
/// which ends the edit.
fn click_opens(renaming: Option<FlowId>, row: FlowId) -> bool {
    renaming != Some(row)
}

/// The colour of what a flow asks; `None` when it asks nothing.
pub(crate) fn attention_color(attention: Attention) -> Option<Rgba> {
    match attention {
        Attention::Nothing => None,
        Attention::Working => Some(theme::accent()),
        Attention::Review => Some(theme::ok()),
        Attention::NeedsYou => Some(theme::warn()),
        Attention::Failed => Some(theme::err()),
    }
}

/// The coloured line on the left of what needs the user: an answer to read,
/// a question, an error.
pub(crate) fn attention_edge(attention: Attention) -> Option<Rgba> {
    match attention {
        Attention::NeedsYou | Attention::Review | Attention::Failed => attention_color(attention),
        Attention::Nothing | Attention::Working => None,
    }
}

/// Its icon, before the title.
pub(crate) fn attention_icon(attention: Attention) -> gpui::Svg {
    let color = attention_color(attention).unwrap_or_else(theme::fg_faint);
    let glyph = match attention {
        Attention::Nothing => Icon::Pending,
        Attention::Working => Icon::Working,
        Attention::Review => Icon::Passed,
        Attention::NeedsYou => Icon::Chat,
        Attention::Failed => Icon::Failed,
    };
    icon(glyph, color, 14.)
}

/// A filled pill for what needs the user — "Needs you", "To review" — at
/// a row's end and on the Flow's boxes; `scale` follows the Flow's zoom.
pub(crate) fn attention_pill(attention: Attention, scale: f32) -> Option<Div> {
    let text = match attention {
        Attention::NeedsYou => "Needs you",
        Attention::Review => "To review",
        Attention::Nothing | Attention::Working | Attention::Failed => return None,
    };
    let color = attention_color(attention)?;
    Some(crate::widgets::pill(text, color, scale))
}

impl Workspace {
    pub(crate) fn row(
        &self,
        c: &FlowView,
        show_project: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = c.flow.id;
        let selected = self.screen == Screen::Flow(id);
        let attention = Attention::of(c.status, c.flow.unread);
        let title = match &self.rename {
            Some((rid, input, _)) if *rid == id => div()
                .flex_1()
                .min_w_0()
                .child(crate::widgets::in_place(
                    Input::new(input).appearance(false),
                ))
                .into_any_element(),
            // Never squeezed away by what sits beside it.
            _ => div()
                .flex_1()
                .min_w(crate::theme::rem(80.))
                .overflow_hidden()
                .text_ellipsis()
                .whitespace_nowrap()
                .when(attention != Attention::Nothing, |s| {
                    s.font_weight(gpui::FontWeight::SEMIBOLD)
                })
                .child(c.flow.title.clone())
                .into_any_element(),
        };
        let (meta, title_prs) = self.row_meta(c, attention, show_project);
        div()
            .id(SharedString::from(format!("conv-{}", id.0)))
            // Under its project, set in from it; pinned, at the pane's edge.
            .mr_1()
            .ml(crate::theme::rem(if show_project { 4. } else { 14. }))
            .pl(crate::theme::rem(6.))
            .pr_2()
            .py(crate::theme::rem(5.))
            .rounded_md()
            // What needs the user shows as a coloured edge too.
            .border_l_2()
            .border_color(attention_edge(attention).unwrap_or(Rgba {
                a: 0.,
                ..theme::accent()
            }))
            .cursor_pointer()
            .when(selected, |s| s.bg(theme::accent_soft()))
            .when(!selected, |s| s.hover(|s| s.bg(theme::surface_2())))
            .flex()
            .flex_col()
            .gap(crate::theme::rem(2.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(if attention == Attention::Working {
                        spinner(
                            SharedString::from(format!("spin-conv-{}", id.0)),
                            theme::accent(),
                            14.,
                        )
                    } else {
                        attention_icon(attention).into_any_element()
                    })
                    .child(title)
                    .children(attention_pill(attention, 1.))
                    .children(title_prs),
            )
            .children(meta)
            .on_click(cx.listener(move |this, _, _, cx| {
                if click_opens(this.rename.as_ref().map(|(r, ..)| *r), id) {
                    this.select(id, cx);
                }
            }))
            .on_mouse_down(
                gpui::MouseButton::Right,
                cx.listener(move |this, e: &gpui::MouseDownEvent, _, cx| {
                    this.row_menu = Some((MenuFor::Flow(id), e.position));
                    cx.notify();
                }),
            )
            .into_any_element()
    }

    /// The line under the title: its project (when pinned), what it is
    /// doing, and its pull requests at the end. A quiet flow says nothing
    /// there: it is one line, its pull requests at the end of its title's
    /// (the second value).
    fn row_meta(
        &self,
        c: &FlowView,
        attention: Attention,
        show_project: bool,
    ) -> (Option<Div>, Option<AnyElement>) {
        let project = show_project
            .then(|| {
                self.snapshot
                    .projects
                    .iter()
                    .find(|p| p.id == c.flow.project)
                    .map(|p| p.name.clone())
            })
            .flatten();
        // What it is doing, never how long ago: times since are noise.
        let what = match attention {
            Attention::Working => Some("Working…"),
            Attention::NeedsYou => Some("Waiting for your answer"),
            Attention::Failed => Some("Stopped with an error"),
            Attention::Review => Some("Done"),
            Attention::Nothing => None,
        };
        let line = [project, what.map(str::to_owned)]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" · ");
        let color = match attention {
            Attention::Nothing | Attention::Review => theme::fg_muted(),
            other => attention_color(other).unwrap_or_else(theme::fg_muted),
        };
        let prs: Vec<leon_domain::PullRequest> =
            self.snapshot.prs_of(c).into_iter().cloned().collect();
        // Its open to-dos, then its pull requests (TODO-4, NAV-6).
        let marks = div()
            .flex()
            .flex_none()
            .items_center()
            .gap(crate::theme::rem(6.))
            .children(crate::todos_view::todo_count(
                &self.snapshot.todos,
                c.flow.id,
            ))
            .children(crate::flow_prs::flow_prs(c.flow.id, prs))
            .into_any_element();
        if line.is_empty() {
            return (None, Some(marks));
        }
        let meta = Some(
            div()
                .pl(crate::theme::rem(22.))
                .flex()
                .items_center()
                .gap(crate::theme::rem(6.))
                .text_size(crate::theme::rem(11.5))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .text_color(color)
                        .child(line),
                )
                .child(marks),
        );
        (meta, None)
    }
}

#[cfg(test)]
mod tests {
    use leon_domain::FlowId;

    use super::click_opens;

    #[test]
    fn a_click_in_the_title_being_edited_keeps_editing() {
        let (renamed, other) = (FlowId(1), FlowId(2));
        // Placing the caret or selecting text in the input.
        assert!(!click_opens(Some(renamed), renamed));
        // Another row opens, which ends the edit.
        assert!(click_opens(Some(renamed), other));
        assert!(click_opens(None, renamed));
    }
}
