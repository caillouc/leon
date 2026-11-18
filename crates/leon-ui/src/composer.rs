//! The message box under a transcript: question card, status line, text,
//! pickers (CONV-3..6, CONV-9).

use gpui::{
    AnyElement, Context, Focusable, IntoElement, SharedString, Window, div, prelude::*, px,
};
use gpui_component::input::{IndentInline, Textarea};
use leon_core::Intent;
use leon_domain::{ConversationId, Status};

use crate::icons::{Icon, icon};
use crate::picker::Look;
use crate::theme;
use crate::widgets::{button, primary};
use crate::workspace::Workspace;

impl Workspace {
    pub(crate) fn render_composer(
        &mut self,
        conversation: ConversationId,
        jump: Option<AnyElement>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(s) = self.snapshot.conversations.get(&conversation).cloned() else {
            return div().into_any_element();
        };
        let running = s.status == Status::Running;
        let mut column = div()
            .flex_none()
            .px_6()
            .pb_4()
            .pt_1()
            .w_full()
            .max_w(px(theme::COLUMN))
            .mx_auto()
            .flex()
            .flex_col()
            .gap_2();

        if let Some(q) = &s.question {
            let options = q.options.iter().enumerate().map(|(ix, option)| {
                let answer = option.clone();
                let question = q.id.clone();
                let make = if ix == 0 { primary } else { button };
                make(
                    SharedString::from(format!("q-{ix}")),
                    option.clone(),
                    cx.listener(move |this, _, _, _| {
                        this.core.send(Intent::Answer {
                            conversation,
                            question: question.clone(),
                            answer: answer.clone(),
                        });
                    }),
                )
            });
            column = column.child(
                div()
                    .p_3()
                    .rounded_lg()
                    .border_1()
                    .border_color(theme::warn())
                    .bg(theme::surface())
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        div()
                            .text_size(px(10.5))
                            .text_color(theme::warn())
                            .child("QUESTION"),
                    )
                    .child(
                        div()
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .child(q.title.clone()),
                    )
                    .when(!q.detail.is_empty(), |d| {
                        d.child(
                            // What waits for approval, e.g. the command: the
                            // most important text of the card.
                            div()
                                .px_2()
                                .py_1()
                                .rounded_md()
                                .bg(theme::surface_2())
                                .text_size(px(12.5))
                                .text_color(theme::fg())
                                .font_family(theme::MONO)
                                .child(q.detail.clone()),
                        )
                    })
                    .child(div().flex().gap_2().children(options)),
            );
        }
        if let Some((_, entry)) = self.rewind.filter(|(id, _)| *id == conversation) {
            let later = self
                .snapshot
                .transcripts
                .get(&conversation)
                .map_or(0, |t| t.entries.len().saturating_sub(entry));
            column = column.child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_3()
                    .py_2()
                    .rounded_lg()
                    .border_1()
                    .border_color(theme::warn())
                    .text_size(px(12.))
                    .child(div().flex_1().child(format!(
                        "Editing an earlier message: sending removes it and the {later} entries after it. Files the agent changed stay as they are."
                    )))
                    .child(button(
                        "rewind-cancel",
                        "Cancel",
                        cx.listener(|this, _, window, cx| {
                            this.rewind = None;
                            this.composer
                                .update(cx, |state, cx| state.set_value("", window, cx));
                            cx.notify();
                        }),
                    )),
            );
        }
        if !s.plan.is_empty() {
            column = column.child(plan_card(&s.plan));
        }
        // Above the message box, from its left: back to the user's last
        // message, then what the agent is doing.
        let status = Self::status_line(&s).map(|line| {
            div()
                .min_w_0()
                .text_size(px(12.))
                .text_color(theme::fg_muted())
                .child(line)
        });
        if jump.is_some() || status.is_some() {
            column = column.child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .children(jump)
                    .children(status),
            );
        }

        let settings = s.conversation.settings.clone();
        let [backend, model, effort, permission] = self.setting_pickers(
            Look::Inline,
            "session",
            &settings,
            cx,
            move |this, next, _, _| {
                this.core.send(Intent::ChangeSettings {
                    conversation,
                    settings: next,
                });
            },
        );
        let agent = self.conversation_name(&s);
        let action = if running {
            button(
                "stop",
                "Stop",
                cx.listener(move |this, _, _, _| this.core.send(Intent::Stop(conversation))),
            )
        } else {
            primary(
                "send",
                "Send",
                cx.listener(|this, _, window, cx| this.send_composer(window, cx)),
            )
        };
        let project = self
            .snapshot
            .flow(s.conversation.flow)
            .map(|c| c.flow.project);
        let slash = self.slash_list(&self.composer.clone(), project, cx);
        let ghost = self.argument_ghost(&self.composer, project, cx);
        let suggestion = (!running && self.rewind.is_none())
            .then(|| s.suggestions.first().cloned())
            .flatten();
        self.show_suggestion(suggestion.as_deref(), window, cx);
        // Tab: the first `/` command while the list shows, else the
        // suggestion in an empty box, else a tab.
        let input_box = Self::paste_aware(div(), &self.composer, cx).capture_action(cx.listener(
            move |this, _: &IndentInline, window, cx| {
                let composer = this.composer.clone();
                if this.complete_slash(&composer, project, window, cx) {
                    cx.stop_propagation();
                } else if let Some(text) = &suggestion {
                    this.accept_suggestion(text, window, cx);
                }
            },
        ));
        column
            .children(slash)
            .child(
                input_box
                    .p_2()
                    .rounded_lg()
                    .border_1()
                    .border_color(theme::border_strong())
                    .bg(theme::surface())
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(Textarea::new(&self.composer).appearance(false))
                    .children(ghost)
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_1()
                            .child(
                                div()
                                    .px_2()
                                    .text_size(px(12.))
                                    .font_weight(gpui::FontWeight::MEDIUM)
                                    .child(agent),
                            )
                            .child(backend)
                            .child(model)
                            .child(effort)
                            .child(permission)
                            .child(div().flex_1())
                            .child(action),
                    ),
            )
            .into_any_element()
    }

    /// Sends the composer's text to the shown conversation (CONV-1).
    pub(crate) fn send_composer(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(conversation) = self.current_conversation() else {
            return;
        };
        if self
            .snapshot
            .conversations
            .get(&conversation)
            .is_some_and(|s| s.status == Status::Running)
        {
            return;
        }
        let text = crate::message_input::message(&self.composer.read(cx).content(), &self.pasted);
        if text.is_empty() {
            return;
        }
        self.pasted.clear();
        match self.rewind.take() {
            Some((id, entry)) if id == conversation => {
                self.core.send(Intent::Rewind {
                    conversation,
                    entry,
                    text,
                });
            }
            _ => self.core.send(Intent::Send { conversation, text }),
        }
        self.composer
            .update(cx, |state, cx| state.set_value("", window, cx));
    }

    /// Puts an earlier message of the user back in the message box; sending
    /// it goes back to before it (CONV-7, CONV-8).
    pub(crate) fn start_edit(
        &mut self,
        conversation: ConversationId,
        entry: usize,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self
            .snapshot
            .conversations
            .get(&conversation)
            .is_some_and(|s| s.status == Status::Running)
        {
            return;
        }
        self.rewind = Some((conversation, entry));
        let text = SharedString::from(text.to_owned());
        self.composer
            .update(cx, |state, cx| state.set_value(text, window, cx));
        self.composer.focus_handle(cx).focus(window, cx);
        cx.notify();
    }
}

/// The agent's plan (CONV-14): its steps, the one being worked on stressed,
/// finished ones dimmed, and how many are done.
fn plan_card(plan: &[leon_domain::PlanStep]) -> AnyElement {
    use leon_domain::StepStatus;
    let done = plan.iter().filter(|s| s.status == StepStatus::Done).count();
    let steps = plan.iter().map(|step| {
        let (glyph, color, text) = match step.status {
            StepStatus::Done => (Icon::Passed, theme::ok(), theme::fg_faint()),
            StepStatus::InProgress => (Icon::Working, theme::accent(), theme::fg()),
            StepStatus::Pending => (Icon::Pending, theme::fg_faint(), theme::fg_muted()),
        };
        div()
            .flex()
            .items_start()
            .gap_2()
            .child(div().pt(px(2.)).child(icon(glyph, color, 13.)))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_color(text)
                    .when(step.status == StepStatus::InProgress, |d| {
                        d.font_weight(gpui::FontWeight::MEDIUM)
                    })
                    .when(step.status == StepStatus::Done, gpui::Styled::line_through)
                    .child(step.text.clone()),
            )
    });
    div()
        .id("plan")
        .max_h(px(180.))
        .overflow_y_scroll()
        .px_3()
        .py_2()
        .flex()
        .flex_col()
        .gap(px(5.))
        .rounded_lg()
        .border_1()
        .border_color(theme::border())
        .bg(theme::panel())
        .text_size(px(12.5))
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .text_size(px(11.5))
                .text_color(theme::fg_muted())
                .child(div().font_weight(gpui::FontWeight::SEMIBOLD).child("Plan"))
                .child(format!("{done}/{} done", plan.len())),
        )
        .children(steps)
        .into_any_element()
}
