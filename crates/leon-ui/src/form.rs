//! New conversation (CONV-2): pick a project, an agent, the CLI and its
//! settings, and write the first message.

use gpui::{AnyElement, Context, IntoElement, Window, div, prelude::*, px};
use gpui_component::input::Textarea;
use leon_core::Intent;

use crate::picker::Choice;
use crate::theme;
use crate::widgets::{button, empty, label, primary};
use crate::workspace::{Screen, Workspace, text_of};

impl Workspace {
    pub(crate) fn render_new_conversation(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if self.snapshot.projects.is_empty() {
            return div()
                .p_6()
                .flex()
                .flex_col()
                .gap_3()
                .child(empty(
                    "Add a project folder first: conversations belong to a project.",
                ))
                .child(div().child(primary(
                    "form-add-project",
                    "Add project folder",
                    cx.listener(|this, _, _, cx| this.add_project(cx)),
                )))
                .into_any_element();
        }
        let project = self
            .form
            .project
            .and_then(|id| self.snapshot.projects.iter().find(|p| p.id == id))
            .or_else(|| self.snapshot.projects.first())
            .cloned();
        let Some(project) = project else {
            return empty("No project.");
        };
        let slash = self.slash_list(&self.form_text.clone(), Some(project.id), cx);
        let chips = self.pasted_chips(cx);
        let input_box = Self::paste_aware(div(), cx);
        let projects: Vec<Choice> = self
            .snapshot
            .projects
            .iter()
            .map(|p| (p.id.0.to_string(), p.name.clone()))
            .collect();
        let project_picker = self.picker(
            "form-project",
            &project.name,
            projects,
            cx,
            |this, value, _, _| {
                this.form.project = value.parse().ok().map(leon_domain::ProjectId);
                this.form.settings.agent = None;
            },
        );
        let agents = self
            .snapshot
            .agents
            .get(&project.id)
            .cloned()
            .unwrap_or_default();
        let agent_choices: Vec<Choice> =
            std::iter::once((String::new(), "CLI's own agent".to_owned()))
                .chain(agents.iter().map(|a| (a.name.clone(), a.name.clone())))
                .collect();
        let agent_label = self
            .form
            .settings
            .agent
            .clone()
            .unwrap_or_else(|| "CLI's own agent".to_owned());
        let description = self
            .form
            .settings
            .agent
            .as_ref()
            .and_then(|name| agents.iter().find(|a| &a.name == name))
            .map_or_default(|a| a.description.clone());
        let agent_picker = self.picker(
            "form-agent",
            &agent_label,
            agent_choices,
            cx,
            |this, value, _, _| {
                this.form.settings.agent = (!value.is_empty()).then_some(value);
            },
        );
        let settings = self.form.settings.clone();
        let pickers = self.setting_pickers("form", &settings, cx, |this, next, _, _| {
            this.form.settings = next;
        });
        let _ = window;
        div()
            .id("new-conversation")
            .size_full()
            .overflow_y_scroll()
            .child(
                div()
                    .max_w(px(720.))
                    .mx_auto()
                    .p_6()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(
                        div()
                            .text_size(px(16.))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child("New conversation"),
                    )
                    .child(label("Project").px_0())
                    .child(div().flex().child(project_picker))
                    .child(label("Agent").px_0())
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_3()
                            .child(agent_picker)
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .text_color(theme::fg_faint())
                                    .child(description),
                            ),
                    )
                    .child(label("CLI and settings").px_0())
                    .child(pickers)
                    .child(label("First message").px_0())
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
                            .child(Textarea::new(&self.form_text)),
                    )
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .child(primary(
                                "form-start",
                                "Start",
                                cx.listener(|this, _, window, cx| {
                                    this.start_conversation(window, cx);
                                }),
                            ))
                            .child(button(
                                "form-cancel",
                                "Cancel",
                                cx.listener(|this, _, _, cx| {
                                    this.screen = Screen::Nothing;
                                    cx.notify();
                                }),
                            )),
                    ),
            )
            .into_any_element()
    }

    pub(crate) fn start_conversation(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = crate::message_input::message(&text_of(&self.form_text, cx), &self.pasted);
        let project = self
            .form
            .project
            .or_else(|| self.snapshot.projects.first().map(|p| p.id));
        let Some(project) = project else { return };
        if text.is_empty() {
            return;
        }
        self.pasted.clear();
        self.core.send(Intent::NewConversation {
            project,
            settings: self.form.settings.clone(),
            text,
        });
        self.form_text
            .update(cx, |state, cx| state.set_value("", window, cx));
        self.screen = Screen::Nothing;
        self.pending_open = true;
        cx.notify();
    }
}
