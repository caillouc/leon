//! New conversation (CONV-2): pick a project, an agent, the CLI and its
//! settings, and write the first message.

use gpui::{AnyElement, Context, IntoElement, Window, div, prelude::*, px};
use gpui_component::input::{IndentInline, Textarea};
use leon_core::Intent;

use crate::icons::{Icon, icon};
use crate::picker::{Choice, Look};
use crate::theme;
use crate::widgets::{button, empty, primary};
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
        let project_id = project.id;
        let slash = self.slash_list(&self.form_text, Some(project_id), cx);
        let ghost = self.argument_ghost(&self.form_text, Some(project_id), cx);
        let chips = self.pasted_chips(cx);
        // Tab writes in the first `/` command while the list shows.
        let input_box = Self::paste_aware(div(), cx).capture_action(cx.listener(
            move |this, _: &IndentInline, window, cx| {
                let input = this.form_text.clone();
                if this.complete_slash(&input, Some(project_id), window, cx) {
                    cx.stop_propagation();
                }
            },
        ));
        let projects: Vec<Choice> = self
            .snapshot
            .projects
            .iter()
            .map(|p| (p.id.0.to_string(), p.name.clone()))
            .collect();
        let project_picker = self.picker_look(
            Look::Field,
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
            std::iter::once((String::new(), "The CLI's own agent".to_owned()))
                .chain(agents.iter().map(|a| (a.name.clone(), a.name.clone())))
                .collect();
        let agent_label = self
            .form
            .settings
            .agent
            .clone()
            .unwrap_or_else(|| "The CLI's own agent".to_owned());
        let description = self
            .form
            .settings
            .agent
            .as_ref()
            .and_then(|name| agents.iter().find(|a| &a.name == name))
            .map_or_else(
                || "No agent file: the CLI as it comes.".to_owned(),
                |a| a.description.clone(),
            );
        let agent_picker = self.picker_look(
            Look::Field,
            "form-agent",
            &agent_label,
            agent_choices,
            cx,
            |this, value, _, _| {
                this.form.settings.agent = (!value.is_empty()).then_some(value);
            },
        );
        let settings = self.form.settings.clone();
        let [backend, model, effort, permission] =
            self.setting_pickers(Look::Field, "form", &settings, cx, |this, next, _, _| {
                this.form.settings = next;
            });
        let _ = window;
        let card = div()
            .w_full()
            .max_w(px(560.))
            .p_4()
            .flex()
            .flex_col()
            .gap_3()
            .rounded_xl()
            .border_1()
            .border_color(theme::border_strong())
            .bg(theme::panel())
            .shadow_lg()
            .child(
                div()
                    .text_size(px(15.))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .child("New conversation"),
            )
            .child(pair(
                field("Project", project_picker),
                field("Agent", agent_picker),
            ))
            // Two lines at most, always the same height: picking another
            // agent does not move the form.
            .child(
                div()
                    .h(px(34.))
                    .text_size(px(11.5))
                    .line_height(px(17.))
                    .text_color(theme::fg_faint())
                    .line_clamp(2)
                    .text_ellipsis()
                    .child(description),
            )
            .child(pair(field("CLI", backend), field("Model", model)))
            .child(pair(field("Effort", effort), field("Permission", permission)))
            .child(
                field(
                    "First message",
                    div()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .children(self.form.issue.as_ref().map(|issue| {
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .text_size(px(12.))
                                .text_color(theme::fg_muted())
                                .child(icon(Icon::Issue, theme::fg_muted(), 13.))
                                .child(format!("For issue {}", issue.text()))
                                .child(
                                    div()
                                        .id("form-unlink-issue")
                                        .cursor_pointer()
                                        .child(icon(Icon::Close, theme::fg_faint(), 12.))
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.form.issue = None;
                                            cx.notify();
                                        })),
                                )
                        }))
                        .children(slash)
                        .child(
                            input_box
                                .p_2()
                                .rounded_md()
                                .border_1()
                                .border_color(theme::border_strong())
                                .bg(theme::surface())
                                .flex()
                                .flex_col()
                                .gap_2()
                                .children(chips)
                                .child(Textarea::new(&self.form_text).appearance(false))
                                .children(ghost),
                        )
                        .into_any_element(),
                ),
            )
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(theme::fg_faint())
                    .child("Agents come from your agent files. No worktree is created here: the agent creates one when it needs it."),
            )
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap_2()
                    .child(button(
                        "form-cancel",
                        "Cancel",
                        cx.listener(|this, _, _, cx| {
                            this.screen = Screen::Nothing;
                            cx.notify();
                        }),
                    ))
                    .child(primary(
                        "form-start",
                        "Start",
                        cx.listener(|this, _, window, cx| {
                            this.start_conversation(window, cx);
                        }),
                    )),
            );
        div()
            .id("new-conversation")
            .size_full()
            .overflow_y_scroll()
            .flex()
            .justify_center()
            .items_start()
            .px_4()
            .pt(px(56.))
            .pb_6()
            .child(card)
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
            issue: self.form.issue.take(),
        });
        self.form_text
            .update(cx, |state, cx| state.set_value("", window, cx));
        self.screen = Screen::Nothing;
        self.pending_open = true;
        cx.notify();
    }
}

/// A labelled form field.
fn field(name: &str, control: AnyElement) -> AnyElement {
    div()
        .flex_1()
        .min_w_0()
        .flex()
        .flex_col()
        .gap(px(5.))
        .child(
            div()
                .text_size(px(12.))
                .text_color(theme::fg_muted())
                .child(name.to_owned()),
        )
        .child(control)
        .into_any_element()
}

/// Two fields side by side.
fn pair(left: AnyElement, right: AnyElement) -> AnyElement {
    div()
        .flex()
        .gap_3()
        .child(left)
        .child(right)
        .into_any_element()
}
