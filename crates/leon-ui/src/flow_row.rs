//! A flow's row in the left pane (NAV-4, NAV-5, NAV-6): what it asks
//! of the user at a glance — working, needs you, to review, failed — its pull
//! request, and the issue it works on, which opens in Leon's issue view.

use gpui::{AnyElement, Context, Div, IntoElement, Rgba, SharedString, div, prelude::*, px};
use gpui_component::input::Input;
use leon_core::{FlowView, Intent};
use leon_domain::{Attention, GhRef};

use crate::icons::{Icon, icon};
use crate::left::{ago, since};
use crate::pr_view::pr_badge;
use crate::row_menu::MenuFor;
use crate::spinner::spinner;
use crate::theme;
use crate::workspace::{Screen, Workspace};

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

/// Its icon, before the title.
fn attention_icon(attention: Attention) -> gpui::Svg {
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
    Some(
        div()
            .flex_none()
            .px(px(6. * scale))
            .rounded_full()
            .bg(Rgba { a: 0.18, ..color })
            .text_size(px(10.5 * scale))
            .font_weight(gpui::FontWeight::SEMIBOLD)
            .text_color(color)
            .child(text),
    )
}

/// The issue it works on, opening Leon's view of it.
fn issue_chip(issue: &GhRef, cx: &mut Context<Workspace>) -> AnyElement {
    let item = issue.clone();
    div()
        .id(SharedString::from(format!("conv-issue-{}", issue.text())))
        .flex()
        .flex_none()
        .items_center()
        .gap(px(3.))
        .px(px(5.))
        .rounded(px(4.))
        .bg(theme::surface_2())
        .text_color(theme::accent())
        .cursor_pointer()
        .hover(|s| s.bg(theme::accent_soft()))
        .child(icon(Icon::Issue, theme::accent(), 11.))
        .child(format!("#{}", issue.number))
        .on_click(cx.listener(move |this, _, _, cx| {
            cx.stop_propagation();
            this.core.send(Intent::ShowDetail(Some(item.clone())));
        }))
        .into_any_element()
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
        let edge = match attention {
            Attention::NeedsYou | Attention::Review | Attention::Failed => {
                attention_color(attention)
            }
            Attention::Nothing | Attention::Working => None,
        };
        let title = match &self.rename {
            Some((rid, input, _)) if *rid == id => div()
                .flex_1()
                .min_w_0()
                .child(Input::new(input))
                .into_any_element(),
            // Never squeezed away by what sits beside it.
            _ => div()
                .flex_1()
                .min_w(px(80.))
                .overflow_hidden()
                .text_ellipsis()
                .whitespace_nowrap()
                .when(attention != Attention::Nothing, |s| {
                    s.font_weight(gpui::FontWeight::SEMIBOLD)
                })
                .child(c.flow.title.clone())
                .into_any_element(),
        };
        div()
            .id(SharedString::from(format!("conv-{}", id.0)))
            .mx_1()
            .pl(px(6.))
            .pr_2()
            .py(px(5.))
            .rounded_md()
            // What needs the user shows as a coloured edge too.
            .border_l_2()
            .border_color(edge.unwrap_or(Rgba {
                a: 0.,
                ..theme::accent()
            }))
            .cursor_pointer()
            .when(selected, |s| s.bg(theme::accent_soft()))
            .when(!selected, |s| s.hover(|s| s.bg(theme::surface_2())))
            .flex()
            .flex_col()
            .gap(px(2.))
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
                    .children(attention_pill(attention, 1.)),
            )
            .child(self.row_meta(c, attention, show_project))
            .children(self.row_refs(c, cx))
            .on_click(cx.listener(move |this, _, _, cx| this.select(id, cx)))
            .on_mouse_down(
                gpui::MouseButton::Right,
                cx.listener(move |this, e: &gpui::MouseDownEvent, _, cx| {
                    this.row_menu = Some((MenuFor::Flow(id), e.position));
                    cx.notify();
                }),
            )
            .into_any_element()
    }

    /// The line under the title: project, agents, what it is doing.
    fn row_meta(&self, c: &FlowView, attention: Attention, show_project: bool) -> Div {
        let project = if show_project {
            self.snapshot
                .projects
                .iter()
                .find(|p| p.id == c.flow.project)
                .map_or_default(|p| format!("{} · ", p.name))
        } else {
            String::new()
        };
        let agents = match c.conversations.len() {
            0 | 1 => String::new(),
            n => format!("{n} agents · "),
        };
        let what = match attention {
            Attention::Working => "Working…".to_owned(),
            Attention::NeedsYou => "Waiting for your answer".to_owned(),
            Attention::Failed => "Stopped with an error".to_owned(),
            Attention::Review => format!("Done {}", since(c.flow.updated_at)),
            Attention::Nothing => ago(c.flow.updated_at),
        };
        let color = match attention {
            Attention::Nothing | Attention::Review => theme::fg_muted(),
            other => attention_color(other).unwrap_or_else(theme::fg_muted),
        };
        div()
            .pl(px(22.))
            .flex()
            .items_center()
            .gap(px(6.))
            .text_size(px(11.5))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .text_color(color)
                    .child(format!("{project}{agents}{what}")),
            )
    }

    /// A last line for the issues and pull requests of the flow, when it has
    /// any (CTX-2): issues first, then pull requests, each opening Leon's
    /// view of it; past a few, how many more.
    fn row_refs(&self, c: &FlowView, cx: &mut Context<Self>) -> Option<Div> {
        const SHOWN: usize = 3;
        let prs = self.snapshot.prs_of(c);
        if c.issues.is_empty() && prs.is_empty() {
            return None;
        }
        let more = |n: usize| {
            (n > SHOWN).then(|| {
                div()
                    .flex_none()
                    .text_color(theme::fg_faint())
                    .child(format!("+{}", n - SHOWN))
            })
        };
        Some(
            div()
                .pl(px(22.))
                .flex()
                .flex_wrap()
                .items_center()
                .gap(px(4.))
                .text_size(px(11.5))
                .children(c.issues.iter().take(SHOWN).map(|i| issue_chip(i, cx)))
                .children(more(c.issues.len()))
                .children(
                    prs.iter()
                        .take(SHOWN)
                        .map(|pr| pr_badge(pr, cx))
                        .collect::<Vec<_>>(),
                )
                .children(more(prs.len())),
        )
    }
}
