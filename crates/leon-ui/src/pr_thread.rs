//! An open review thread of a pull request, as on GitHub (GH-3, GH-8): the
//! code it is on, its comments, and answering or resolving it from Leon.

use gpui::{Context, Div, Focusable, FontWeight, SharedString, div, prelude::*};
use gpui_component::input::Textarea;
use leon_core::Intent;
use leon_domain::{Comment, GhRef, PullRequest, ReviewThread};

use crate::icons::{Icon, icon};
use crate::left::since;
use crate::pr_page::{avatar, tint};
use crate::theme;
use crate::widgets::{button, primary};
use crate::workspace::{Workspace, text_of};

impl Workspace {
    /// An open review thread: the file and line it is on, the code there,
    /// then each comment with its author's avatar, and how to answer it.
    pub(crate) fn thread_card(
        &self,
        id: &str,
        pr: &PullRequest,
        thread: &ReviewThread,
        cx: &mut Context<Self>,
    ) -> Div {
        let last = thread.code.len().saturating_sub(1);
        div()
            .rounded_lg()
            .border_1()
            .border_color(theme::border_strong())
            .bg(theme::panel())
            .overflow_hidden()
            .child(
                div()
                    .px_3()
                    .py(crate::theme::rem(7.))
                    .flex()
                    .items_center()
                    .gap_2()
                    .bg(theme::surface_2())
                    .border_b_1()
                    .border_color(theme::border())
                    .child(icon(Icon::Chat, theme::warn(), 13.))
                    .child(
                        div()
                            .min_w_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .font_family(theme::MONO)
                            .text_size(crate::theme::rem(12.))
                            .child(thread.path.clone()),
                    )
                    .children(thread.line.map(|line| {
                        div()
                            .flex_none()
                            .text_size(crate::theme::rem(12.))
                            .text_color(theme::fg_muted())
                            .child(format!("line {line}"))
                    }))
                    .child(div().flex_1())
                    .when(thread.line.is_none(), |d| {
                        d.child(
                            div()
                                .px(crate::theme::rem(7.))
                                .rounded_full()
                                .border_1()
                                .border_color(tint(theme::warn(), 0.6))
                                .text_size(crate::theme::rem(11.))
                                .text_color(theme::warn())
                                .child("Outdated"),
                        )
                    }),
            )
            .when(!thread.code.is_empty(), |d| {
                d.child(
                    div()
                        .py_1()
                        .bg(theme::window())
                        .border_b_1()
                        .border_color(theme::border())
                        .font_family(theme::MONO)
                        .text_size(crate::theme::rem(11.5))
                        .children(
                            thread
                                .code
                                .iter()
                                .enumerate()
                                .map(|(ix, line)| code_line(line, ix == last)),
                        ),
                )
            })
            .children(
                thread.comments.iter().enumerate().map(|(c, comment)| {
                    self.thread_comment(&format!("{id}-{c}"), comment, c > 0, cx)
                }),
            )
            .child(self.thread_footer(id, pr, thread, cx))
    }

    /// Under a thread: answer it, resolve it (GH-8), or open it on GitHub;
    /// while answering, the answer box.
    fn thread_footer(
        &self,
        id: &str,
        pr: &PullRequest,
        thread: &ReviewThread,
        cx: &mut Context<Self>,
    ) -> Div {
        let item = GhRef::pull_request(&pr.repo, pr.number);
        let footer = div()
            .px_3()
            .py(crate::theme::rem(7.))
            .border_t_1()
            .border_color(theme::border())
            .flex()
            .gap_2();
        if self.right.replying.as_deref() == Some(thread.id.as_str()) {
            let input = self.right.thread_reply.clone();
            let thread_id = thread.id.clone();
            return footer
                .flex_col()
                .child(
                    div()
                        .rounded_md()
                        .border_1()
                        .border_color(theme::border_strong())
                        .bg(theme::window())
                        .p_1()
                        .child(Textarea::new(&input).appearance(false)),
                )
                .child(
                    div()
                        .flex()
                        .justify_end()
                        .gap_2()
                        .child(button(
                            SharedString::from(format!("{id}-cancel")),
                            "Cancel",
                            cx.listener(|this, _, _, cx| {
                                this.right.replying = None;
                                cx.notify();
                            }),
                        ))
                        .child(primary(
                            SharedString::from(format!("{id}-send")),
                            "Reply",
                            cx.listener(move |this, _, window, cx| {
                                let body = text_of(&input, cx);
                                if body.trim().is_empty() {
                                    return;
                                }
                                this.core.send(Intent::ReplyToThread {
                                    pr: item.clone(),
                                    thread: thread_id.clone(),
                                    body: body.trim().to_owned(),
                                });
                                this.right.replying = None;
                                input.update(cx, |state, cx| state.set_value("", window, cx));
                                cx.notify();
                            }),
                        )),
                );
        }
        let reply_to = thread.id.clone();
        let resolve = thread.id.clone();
        let url = pr.url.clone();
        footer
            .items_center()
            .when(!thread.id.is_empty(), |d| {
                d.child(button(
                    SharedString::from(format!("{id}-reply")),
                    "Reply",
                    cx.listener(move |this, _, window, cx| {
                        this.right.replying = Some(reply_to.clone());
                        this.right.thread_reply.focus_handle(cx).focus(window, cx);
                        cx.notify();
                    }),
                ))
            })
            .when(thread.can_resolve, |d| {
                let item = GhRef::pull_request(&pr.repo, pr.number);
                d.child(button(
                    SharedString::from(format!("{id}-resolve")),
                    "Resolve conversation",
                    cx.listener(move |this, _, _, _| {
                        this.core.send(Intent::ResolveThread {
                            pr: item.clone(),
                            thread: resolve.clone(),
                        });
                    }),
                ))
            })
            .child(div().flex_1())
            .child(
                div()
                    .id(SharedString::from(format!("{id}-github")))
                    .flex()
                    .items_center()
                    .gap(crate::theme::rem(5.))
                    .text_size(crate::theme::rem(12.))
                    .text_color(theme::accent())
                    .cursor_pointer()
                    .child(icon(Icon::External, theme::accent(), 12.))
                    .child("GitHub")
                    .on_click(move |_, _, cx| cx.open_url(&url)),
            )
    }

    fn thread_comment(
        &self,
        id: &str,
        comment: &Comment,
        divided: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        div()
            .px_3()
            .py_2()
            .flex()
            .items_start()
            .gap_2()
            .when(divided, |d| d.border_t_1().border_color(theme::border()))
            .child(avatar(&comment.author, 22.))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(crate::theme::rem(2.))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(crate::theme::rem(5.))
                            .text_size(crate::theme::rem(12.5))
                            .child(
                                div()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(comment.author.clone()),
                            )
                            .child(div().text_color(theme::fg_faint()).child(since(comment.at))),
                    )
                    .child(
                        div()
                            .text_size(crate::theme::rem(13.))
                            .child(self.rich_markdown(id, &comment.body, cx)),
                    ),
            )
    }
}

/// One code line of a review thread: its diff sign, green or red as GitHub
/// colours it; `last` is the commented line.
fn code_line(line: &str, last: bool) -> Div {
    let (sign, text) = line.split_at(line.chars().next().map_or(0, char::len_utf8));
    let (bg, sign_color) = match sign {
        "+" => (Some(theme::add_bg()), theme::ok()),
        "-" => (Some(theme::del_bg()), theme::err()),
        _ => (None, theme::fg_faint()),
    };
    div()
        .flex()
        .when_some(bg, Styled::bg)
        // Every line keeps the edge, so the commented one's text stays put.
        .border_l_2()
        .border_color(if last {
            theme::accent()
        } else {
            tint(theme::accent(), 0.)
        })
        .child(
            div()
                .w(crate::theme::rem(22.))
                .flex_none()
                .text_center()
                .text_color(sign_color)
                .child(sign.to_owned()),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .child(text.to_owned()),
        )
}
