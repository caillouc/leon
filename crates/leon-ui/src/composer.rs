//! The message box under a transcript: question card, status line, text,
//! pickers (CONV-3..6, CONV-9).

use gpui::{
    AnyElement, Context, Focusable, IntoElement, SharedString, Window, div, prelude::*, px,
};
use gpui_component::input::Textarea;
use leon_core::Intent;
use leon_domain::{AgentSessionId, BackendKind, SessionSettings, Status};

use crate::picker::Choice;
use crate::theme;
use crate::widgets::{button, primary};
use crate::workspace::{Workspace, text_of};

impl Workspace {
    pub(crate) fn render_composer(
        &mut self,
        session: AgentSessionId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(s) = self.snapshot.sessions.get(&session).cloned() else {
            return div().into_any_element();
        };
        let running = s.status == Status::Running;
        let mut column = div()
            .flex_none()
            .px_6()
            .pb_4()
            .pt_1()
            .w_full()
            .max_w(px(860.))
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
                            session,
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
                            div()
                                .text_size(px(12.))
                                .text_color(theme::fg_muted())
                                .font_family(theme::MONO)
                                .child(q.detail.clone()),
                        )
                    })
                    .child(div().flex().gap_2().children(options)),
            );
        }
        if !running && !s.suggestions.is_empty() && self.rewind.is_none() {
            let chips = s.suggestions.iter().enumerate().map(|(ix, text)| {
                let text = text.clone();
                div()
                    .id(("suggestion", ix))
                    .px_3()
                    .py(px(3.))
                    .rounded_full()
                    .border_1()
                    .border_color(theme::border_strong())
                    .text_size(px(12.5))
                    .cursor_pointer()
                    .hover(|s| s.bg(theme::surface_2()))
                    .child(text.clone())
                    .on_click(cx.listener(move |this, _, _, _| {
                        this.core.send(Intent::Send {
                            session,
                            text: text.clone(),
                        });
                    }))
            });
            column = column.child(div().flex().flex_wrap().gap_2().children(chips));
        }
        if let Some((_, entry)) = self.rewind.filter(|(id, _)| *id == session) {
            let later = self
                .snapshot
                .transcripts
                .get(&session)
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
        if let Some(line) = Self::status_line(&s) {
            column = column.child(
                div()
                    .text_size(px(12.))
                    .text_color(theme::fg_muted())
                    .child(line),
            );
        }

        let settings = s.session.settings.clone();
        let pickers = self.setting_pickers("session", &settings, cx, move |this, next, _, _| {
            this.core.send(Intent::ChangeSettings {
                session,
                settings: next,
            });
        });
        let agent = self.session_name(&s);
        let action = if running {
            button(
                "stop",
                "Stop",
                cx.listener(move |this, _, _, _| this.core.send(Intent::Stop(session))),
            )
        } else {
            primary(
                "send",
                "Send",
                cx.listener(|this, _, window, cx| this.send_composer(window, cx)),
            )
        };
        let _ = window;
        let project = self
            .snapshot
            .conversation(s.session.conversation)
            .map(|c| c.conversation.project);
        let slash = self.slash_list(&self.composer.clone(), project, cx);
        let chips = self.pasted_chips(cx);
        let input_box = Self::paste_aware(div(), cx);
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
                    .children(chips)
                    .child(Textarea::new(&self.composer))
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
                            .child(pickers)
                            .child(div().flex_1())
                            .child(action),
                    ),
            )
            .into_any_element()
    }

    /// Backend, model, effort and permission pickers; `on_change` gets the
    /// new settings.
    pub(crate) fn setting_pickers(
        &self,
        prefix: &str,
        settings: &SessionSettings,
        cx: &mut Context<Self>,
        on_change: impl Fn(&mut Self, SessionSettings, &mut Window, &mut Context<Self>)
        + Clone
        + 'static,
    ) -> AnyElement {
        let options = self.backend_options(settings.backend);
        let backends: Vec<Choice> = BackendKind::ALL
            .into_iter()
            .filter(|b| !self.backend_options(*b).permissions.is_empty())
            .map(|b| (b.id().to_owned(), b.display_name().to_owned()))
            .collect();
        let with_default = |items: &[String]| -> Vec<Choice> {
            std::iter::once((String::new(), "default".to_owned()))
                .chain(items.iter().map(|m| (m.clone(), m.clone())))
                .collect()
        };
        let s = settings.clone();
        let backend = {
            let (s, f) = (s.clone(), on_change.clone());
            self.picker(
                &format!("{prefix}-backend"),
                settings.backend.display_name(),
                backends,
                cx,
                move |this, value, w, cx| {
                    let Some(kind) = BackendKind::from_id(&value) else {
                        return;
                    };
                    let opts = this.backend_options(kind);
                    let next = SessionSettings {
                        backend: kind,
                        model: None,
                        effort: None,
                        permission: opts
                            .permissions
                            .first()
                            .cloned()
                            .unwrap_or_else(|| "auto".to_owned()),
                        ..s.clone()
                    };
                    f(this, next, w, cx);
                },
            )
        };
        let model = {
            let (s, f) = (s.clone(), on_change.clone());
            let current = settings
                .model
                .clone()
                .unwrap_or_else(|| "default model".to_owned());
            self.picker(
                &format!("{prefix}-model"),
                &current,
                with_default(&options.models),
                cx,
                move |this, value, w, cx| {
                    f(
                        this,
                        SessionSettings {
                            model: (!value.is_empty()).then_some(value),
                            ..s.clone()
                        },
                        w,
                        cx,
                    );
                },
            )
        };
        let effort = {
            let (s, f) = (s.clone(), on_change.clone());
            let current = settings
                .effort
                .as_ref()
                .map_or_else(|| "default effort".to_owned(), |e| format!("effort {e}"));
            self.picker(
                &format!("{prefix}-effort"),
                &current,
                with_default(&options.efforts),
                cx,
                move |this, value, w, cx| {
                    f(
                        this,
                        SessionSettings {
                            effort: (!value.is_empty()).then_some(value),
                            ..s.clone()
                        },
                        w,
                        cx,
                    );
                },
            )
        };
        let permission = {
            let (s, f) = (s, on_change);
            let choices = options
                .permissions
                .iter()
                .map(|p| (p.clone(), p.clone()))
                .collect();
            self.picker(
                &format!("{prefix}-permission"),
                &settings.permission,
                choices,
                cx,
                move |this, value, w, cx| {
                    f(
                        this,
                        SessionSettings {
                            permission: value,
                            ..s.clone()
                        },
                        w,
                        cx,
                    );
                },
            )
        };
        div()
            .flex()
            .items_center()
            .gap_1()
            .child(backend)
            .child(model)
            .child(effort)
            .child(permission)
            .into_any_element()
    }

    /// Sends the composer's text to the shown session (CONV-1).
    pub(crate) fn send_composer(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(session) = self.current_session() else {
            return;
        };
        if self
            .snapshot
            .sessions
            .get(&session)
            .is_some_and(|s| s.status == Status::Running)
        {
            return;
        }
        let text = crate::message_input::message(&text_of(&self.composer, cx), &self.pasted);
        if text.is_empty() {
            return;
        }
        self.pasted.clear();
        match self.rewind.take() {
            Some((id, entry)) if id == session => {
                self.core.send(Intent::Rewind {
                    session,
                    entry,
                    text,
                });
            }
            _ => self.core.send(Intent::Send { session, text }),
        }
        self.composer
            .update(cx, |state, cx| state.set_value("", window, cx));
    }

    /// Puts an earlier message of the user back in the message box; sending
    /// it goes back to before it (CONV-7, CONV-8).
    pub(crate) fn start_edit(
        &mut self,
        session: AgentSessionId,
        entry: usize,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self
            .snapshot
            .sessions
            .get(&session)
            .is_some_and(|s| s.status == Status::Running)
        {
            return;
        }
        self.rewind = Some((session, entry));
        let text = SharedString::from(text.to_owned());
        self.composer
            .update(cx, |state, cx| state.set_value(text, window, cx));
        self.composer.focus_handle(cx).focus(window, cx);
        cx.notify();
    }
}
