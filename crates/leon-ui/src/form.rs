//! A new flow (CONV-2), or another conversation in a flow (FLOW-7), as a
//! clean start page: Leon's logo, the message box with the agent, CLI and
//! settings in its row, and under it the project (or one of the flow's
//! folders) and the issue it is for.

use gpui::{AnyElement, Context, IntoElement, Window, div, img, prelude::*};
use gpui_component::input::IndentInline;
use leon_core::Intent;
use leon_domain::FlowId;

use crate::icons::{Icon, icon};
use crate::picker::{Choice, Look};
use crate::theme;
use crate::widgets::{empty, primary};
use crate::workspace::{Screen, Tab, Workspace};

/// Leon's logo at the top of the page: the bicorne of its app icon, alone
/// (assets/leon.svg), 1056 × 402.
const LOGO: &str = "leon-hat.png";
/// The page's column (design.md, Layout).
const FORM: f32 = 680.;

impl Workspace {
    /// The form: a new flow, or with `flow`, another conversation in it.
    pub(crate) fn render_new_flow(
        &mut self,
        flow: Option<FlowId>,
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
                    "Add a project folder first: flows belong to a project.",
                ))
                .child(div().child(primary(
                    "form-add-project",
                    "Add project folder",
                    cx.listener(|this, _, _, cx| this.add_project(cx)),
                )))
                .into_any_element();
        }
        let in_flow = flow.and_then(|id| self.snapshot.flow(id)).cloned();
        let project = in_flow
            .as_ref()
            .map(|view| view.flow.project)
            .or(self.form.project)
            .and_then(|id| self.snapshot.projects.iter().find(|p| p.id == id))
            .or_else(|| self.snapshot.projects.first())
            .cloned();
        let Some(project) = project else {
            return empty("No project.");
        };
        let project_id = project.id;
        let slash = self.slash_list(&self.form_text, Some(project_id), cx);
        let ghost = self.argument_ghost(&self.form_text, Some(project_id), cx);
        // Tab writes in the first `/` command while the list shows.
        let input_box = Self::paste_aware(div(), &self.form_text, cx).capture_action(cx.listener(
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
            Look::Inline,
            "form-project",
            &project.name,
            projects,
            cx,
            |this, value, _, _| {
                this.form.project = value.parse().ok().map(leon_domain::ProjectId);
                this.form.settings.agent = None;
                this.default_agent();
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
        let chosen = self
            .form
            .settings
            .agent
            .as_ref()
            .and_then(|name| agents.iter().find(|a| &a.name == name));
        let description = chosen.map_or_else(
            || "No agent file: the CLI as it comes.".to_owned(),
            |a| {
                // A model no CLI offers: the CLI and model below run it (EXT-6).
                let unknown = a.model.as_deref().filter(|m| {
                    *m != "inherit" && self.agent_settings(Some(m)).is_none()
                });
                match unknown {
                    Some(model) => format!(
                        "{} (Its model {model} is offered by no CLI: the CLI and model below run it.)",
                        a.description
                    ),
                    None => a.description.clone(),
                }
            },
        );
        let agent_picker = self.picker_look(
            Look::Inline,
            "form-agent",
            &agent_label,
            agent_choices,
            cx,
            move |this, value, _, _| {
                this.form.settings.agent = (!value.is_empty()).then_some(value.clone());
                // Its file's model picks the CLI and the model (EXT-6).
                let model = agents
                    .iter()
                    .find(|a| a.name == value)
                    .and_then(|a| a.model.clone());
                if let Some(next) = this.agent_settings(model.as_deref()) {
                    this.form.settings = next;
                }
            },
        );
        let settings = self.form.settings.clone();
        let [backend, model, effort, permission] =
            self.setting_pickers(Look::Inline, "form", &settings, cx, |this, next, _, _| {
                this.form.settings = next;
            });
        let _ = window;
        let start = primary(
            "form-start",
            "Start",
            cx.listener(|this, _, window, cx| this.submit_form(window, cx)),
        )
        .into_any_element();
        let message = crate::composer::message_box(
            input_box,
            &self.form_text,
            ghost,
            [agent_picker, backend, model, effort, permission],
            start,
        );
        // Where it starts, and for which issue: under the box, quiet.
        let place = match &in_flow {
            Some(view) => self.form_folder_picker(view, cx),
            None => project_picker,
        };
        let issue = self
            .form
            .issue
            .as_ref()
            .filter(|_| flow.is_none())
            .map(|issue| {
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .text_size(crate::theme::rem(12.))
                    .text_color(theme::fg_muted())
                    .child(icon(
                        Icon::Issue,
                        self.known_issue(issue)
                            .map_or(theme::fg_muted(), |i| self.issue_color(i.status.as_ref())),
                        13.,
                    ))
                    .child(format!("For issue {}", issue.short()))
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
            });
        let under = div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap_3()
            .px_1()
            .child(
                div()
                    .flex()
                    .items_center()
                    .child(icon(Icon::Folder, theme::fg_muted(), 14.))
                    .child(place),
            )
            .children(issue);
        let column = div()
            .flex()
            .flex_col()
            .items_center()
            .gap_5()
            .child(
                img(LOGO)
                    .w(crate::theme::rem(160.))
                    .h(crate::theme::rem(61.))
                    .flex_none()
                    // Well above the box, not sitting on it.
                    .mb(crate::theme::rem(64.)),
            )
            .children(in_flow.as_ref().map(|view| {
                div()
                    .text_size(crate::theme::rem(13.))
                    .text_color(theme::fg_muted())
                    .child(format!("New conversation in {}", view.flow.title))
            }))
            .child(
                div()
                    .w_full()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .children(slash)
                    .child(message)
                    .child(under)
                    .child(
                        // The agent's own words; two lines at most, always
                        // the same height: picking another does not move
                        // the box.
                        div()
                            .h(crate::theme::rem(34.))
                            .px_1()
                            .text_size(crate::theme::rem(11.5))
                            .line_height(crate::theme::rem(17.))
                            .text_color(theme::fg_faint())
                            .line_clamp(2)
                            .text_ellipsis()
                            .child(description),
                    ),
            );
        div()
            .id("new-conversation")
            .when(flow.is_none(), gpui::Styled::size_full)
            .when(flow.is_some(), |d| d.flex_1().min_h_0().w_full())
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .justify_center()
            .py_6()
            .child(crate::widgets::centered(FORM, div().px_6().child(column)))
            .into_any_element()
    }

    /// Picks the default agent (Settings) when none is picked and the
    /// form's project has it, with the CLI and model its file asks for.
    pub(crate) fn default_agent(&mut self) {
        let Some(name) = self.snapshot.settings.defaults.agent.clone() else {
            return;
        };
        if self.form.settings.agent.is_some() {
            return;
        }
        let agent = self
            .form
            .project
            .and_then(|p| self.snapshot.agents.get(&p))
            .and_then(|list| list.iter().find(|a| a.name == name));
        let Some(agent) = agent else {
            return;
        };
        let model = agent.model.clone();
        self.form.settings.agent = Some(name);
        if let Some(next) = self.agent_settings(model.as_deref()) {
            self.form.settings = next;
        }
    }

    /// The form's settings for an agent file's `model`, as the user's CLI
    /// order picks them (EXT-6); `None` when the model picks nothing.
    fn agent_settings(&self, model: Option<&str>) -> Option<leon_domain::ConversationSettings> {
        leon_core::settings_for_agent(
            &self.form.settings,
            model,
            &self.snapshot.settings.cli_order,
            &self.snapshot.options,
        )
    }

    /// The folder picker of a new conversation in `view`'s flow: one of its
    /// folders, the one the right pane shows first.
    fn form_folder_picker(
        &mut self,
        view: &leon_core::FlowView,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let picked = self.form_folder(view.flow.id);
        let name = |folder: &std::path::Path| {
            crate::right_folders::folder_name(folder, self.snapshot.git.get(folder))
        };
        let choices: Vec<Choice> = view
            .folders
            .iter()
            .map(|f| {
                (
                    f.display().to_string(),
                    format!("{} · {}", name(f), f.display()),
                )
            })
            .collect();
        let current = picked.as_deref().map_or_default(name);
        self.picker_look(
            Look::Inline,
            "form-folder",
            &current,
            choices,
            cx,
            |this, value, _, _| this.form.folder = Some(value.into()),
        )
    }

    /// The folder a new conversation in `flow` starts in: the one picked,
    /// else the one the right pane shows.
    fn form_folder(&self, flow: FlowId) -> Option<std::path::PathBuf> {
        let view = self.snapshot.flow(flow)?;
        self.form
            .folder
            .clone()
            .filter(|f| view.folders.contains(f))
            .or_else(|| self.folder(flow))
    }

    /// Opens the form for another conversation in `flow` (FLOW-7), with
    /// the default agent.
    pub(crate) fn open_new_in_flow(&mut self, flow: FlowId, cx: &mut Context<Self>) {
        self.form.project = self.snapshot.flow(flow).map(|v| v.flow.project);
        self.form.folder = None;
        self.form.settings.agent = None;
        self.default_agent();
        self.tabs.insert(flow, Tab::New);
        cx.notify();
    }

    /// Start, or Enter in the form's message: a new flow, or a new
    /// conversation in the flow whose form is shown.
    pub(crate) fn submit_form(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match self.screen {
            Screen::Flow(id) if self.tabs.get(&id) == Some(&Tab::New) => {
                self.start_in_flow(id, window, cx);
            }
            _ => self.start_flow(window, cx),
        }
    }

    /// Starts another conversation in `flow`; its tab opens once it exists.
    fn start_in_flow(&mut self, flow: FlowId, window: &mut Window, cx: &mut Context<Self>) {
        let text = crate::message_input::message(&self.form_text.read(cx).content(), &self.pasted);
        let (Some(working_dir), Some(view)) = (self.form_folder(flow), self.snapshot.flow(flow))
        else {
            return;
        };
        if text.is_empty() {
            return;
        }
        let before = view.conversations.len();
        self.pasted.clear();
        self.core.send(Intent::NewConversation {
            flow,
            settings: self.form.settings.clone(),
            working_dir,
            text,
        });
        self.form_text
            .update(cx, |state, cx| state.set_value("", window, cx));
        self.tabs.insert(flow, Tab::Flow);
        self.pending_conversation = Some((flow, before));
        cx.notify();
    }

    pub(crate) fn start_flow(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = crate::message_input::message(&self.form_text.read(cx).content(), &self.pasted);
        let project = self
            .form
            .project
            .or_else(|| self.snapshot.projects.first().map(|p| p.id));
        let Some(project) = project else { return };
        if text.is_empty() {
            return;
        }
        self.pasted.clear();
        self.core.send(Intent::NewFlow {
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
