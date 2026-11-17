//! The message box under a transcript: question card, status line, text,
//! pickers (CONV-3..6, CONV-9).

use gpui::{AnyElement, Context, IntoElement, SharedString, Window, div, prelude::*, px};
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
        column
            .child(
                div()
                    .p_2()
                    .rounded_lg()
                    .border_1()
                    .border_color(theme::border_strong())
                    .bg(theme::surface())
                    .flex()
                    .flex_col()
                    .gap_2()
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
        let text = text_of(&self.composer, cx);
        if text.trim().is_empty() {
            return;
        }
        self.core.send(Intent::Send { session, text });
        self.composer
            .update(cx, |state, cx| state.set_value("", window, cx));
    }
}
