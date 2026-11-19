//! The window's root view: owns the UI-only state (what is selected, which
//! picker is open) and renders the three panes from the core's snapshot.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Instant;

use gpui::{
    AnyElement, App, Context, Entity, Focusable, FollowMode, IntoElement, ListAlignment, ListState,
    Render, SharedString, Subscription, SystemNotification, Task, Window, div, prelude::*, px,
};
use gpui_component::input::{InputEvent, InputState, TextareaState};
use leon_core::{CoreHandle, Intent, Snapshot};
use leon_domain::{BackendKind, ConversationId, ConversationSettings, FlowId, ProjectId};

use crate::flow::FlowState;
use crate::github::GhList;
use crate::panes::Side;
use crate::right::RightState;
use crate::theme;

/// What the main pane shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Screen {
    Nothing,
    Flow(FlowId),
    NewFlow,
    /// My issues or my pull requests (GH-2, GH-3).
    GitHub(GhList),
    /// What works and what needs setting up.
    Doctor,
    /// What each CLI is connected to (TOOLS-1).
    Tools,
    /// The user's settings (SETTINGS-1).
    Settings,
}

/// The tab shown for a flow (FLOW-3): its diagram, one of its conversations,
/// or the form starting another.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Tab {
    Flow,
    Conversation(ConversationId),
    /// The form starting another conversation in the flow (FLOW-7).
    New,
}

/// Choices in the new-flow form.
#[derive(Debug, Clone)]
pub(crate) struct NewForm {
    pub project: Option<ProjectId>,
    pub settings: ConversationSettings,
    /// The issue the flow is started for (GH-2).
    pub issue: Option<leon_domain::GhRef>,
    /// In a flow: the folder picked for the new conversation (FLOW-7).
    pub folder: Option<std::path::PathBuf>,
}

/// Root view of the window.
pub struct Workspace {
    pub(crate) core: CoreHandle,
    pub(crate) snapshot: Arc<Snapshot>,
    pub(crate) screen: Screen,
    pub(crate) tabs: HashMap<FlowId, Tab>,
    pub(crate) composer: Entity<TextareaState>,
    /// The workspace's own focus, so shortcuts work with no text box focused.
    pub(crate) focus: gpui::FocusHandle,
    /// What the message box shows while empty: the suggestion, if any.
    pub(crate) composer_placeholder: SharedString,
    pub(crate) form_text: Entity<TextareaState>,
    pub(crate) search: Entity<InputState>,
    /// The flow being renamed in the left pane, its input, and the
    /// subscription that saves on Enter.
    pub(crate) rename: Option<(FlowId, Entity<InputState>, Subscription)>,
    /// The left-pane row whose right-click menu is open, and where.
    pub(crate) row_menu: Option<(crate::row_menu::MenuFor, gpui::Point<gpui::Pixels>)>,
    pub(crate) form: NewForm,
    pub(crate) open_picker: Option<SharedString>,
    pub(crate) show_archived: bool,
    /// Tool calls and long messages the user unfolded.
    pub(crate) expanded: HashSet<(ConversationId, usize)>,
    /// Long pastes waiting in the message box (CONV-12).
    pub(crate) pasted: Vec<String>,
    /// The user's message being edited: sending goes back to before it
    /// (CONV-7, CONV-8).
    pub(crate) rewind: Option<(ConversationId, usize)>,
    pub(crate) lists: HashMap<ConversationId, (ListState, usize)>,
    pub(crate) right: RightState,
    pub(crate) flow: FlowState,
    pub(crate) panes: crate::panes::Panes,
    /// GitHub's folded sections the user opened.
    pub(crate) open_details: crate::markdown::OpenDetails,
    pub(crate) gh: crate::github::GhState,
    pub(crate) tools_form: crate::tools_view::ToolsForm,
    pub(crate) settings_form: crate::settings_view::SettingsForm,
    pub(crate) flash: crate::flash::Flash,
    /// Open the next new flow when it shows up.
    pub(crate) pending_open: bool,
    /// A flow and how many conversations it had when the user started one
    /// in it: its tab opens when it shows up (FLOW-7).
    pub(crate) pending_conversation: Option<(FlowId, usize)>,
    /// The flow last said to be read (an agent's tab on screen).
    pub(crate) reading: Option<ConversationId>,
    shown_notifications: u64,
    started: Option<Instant>,
    _updates: Task<()>,
    _subscriptions: Vec<Subscription>,
}

impl Workspace {
    /// Builds the view and follows the core's snapshots.
    pub fn new(
        core: &CoreHandle,
        started: Instant,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.set_global(crate::markdown::Core(core.clone()));
        let mut snapshots = core.subscribe();
        let updates = cx.spawn(async move |this, cx| {
            while snapshots.changed().await.is_ok() {
                let next = Arc::clone(&snapshots.borrow_and_update());
                if this
                    .update(cx, |view, cx| view.on_snapshot(next, cx))
                    .is_err()
                {
                    break;
                }
            }
        });
        let composer = cx.new(|cx| {
            TextareaState::new(window, cx)
                .auto_grow(1, 10)
                .submit_on_enter(true)
                .placeholder(crate::message_input::PLACEHOLDER)
        });
        let form_text = cx.new(|cx| {
            TextareaState::new(window, cx)
                .auto_grow(3, 12)
                .submit_on_enter(true)
                .placeholder("What should the agent do?")
        });
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search flows"));
        let (right, right_subscriptions) = RightState::new(window, cx);
        let tools_form = crate::tools_view::ToolsForm::new(window, cx);
        let settings_form = crate::settings_view::SettingsForm::new(window, cx);
        let mut subscriptions = vec![
            cx.subscribe_in(
                &composer,
                window,
                |this, _, event: &InputEvent, window, cx| {
                    if let InputEvent::PressEnter { shift: false, .. } = event {
                        this.send_composer(window, cx);
                    }
                },
            ),
            cx.subscribe_in(
                &form_text,
                window,
                |this, _, event: &InputEvent, window, cx| {
                    if let InputEvent::PressEnter { shift: false, .. } = event {
                        this.submit_form(window, cx);
                    }
                },
            ),
            cx.subscribe(&search, |_, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    cx.notify();
                }
            }),
        ];
        subscriptions.extend(right_subscriptions);
        // An answer counts as seen only while this window is the active one.
        subscriptions.push(cx.observe_window_activation(window, |this, window, _| {
            this.core
                .send(Intent::WindowActive(window.is_window_active()));
        }));
        let snapshot = core.snapshot();
        let defaults = snapshot.settings.defaults.clone();
        let gh = crate::github::GhState::new(window, cx);
        Self {
            core: core.clone(),
            snapshot,
            screen: Screen::Nothing,
            tabs: HashMap::new(),
            composer,
            focus: cx.focus_handle(),
            composer_placeholder: crate::message_input::PLACEHOLDER.into(),
            form_text,
            search,
            rename: None,
            row_menu: None,
            form: NewForm {
                project: None,
                settings: ConversationSettings {
                    backend: defaults.backend,
                    agent: None,
                    model: defaults.model,
                    effort: defaults.effort,
                    permission: defaults.permission,
                },
                issue: None,
                folder: None,
            },
            open_picker: None,
            show_archived: false,
            expanded: HashSet::new(),
            pasted: Vec::new(),
            rewind: None,
            lists: HashMap::new(),
            right,
            flow: FlowState::default(),
            panes: crate::panes::Panes::default(),
            open_details: crate::markdown::OpenDetails::default(),
            gh,
            tools_form,
            settings_form,
            flash: crate::flash::Flash::default(),
            pending_open: false,
            pending_conversation: None,
            reading: None,
            shown_notifications: 0,
            started: Some(started),
            _updates: updates,
            _subscriptions: subscriptions,
        }
    }

    fn on_snapshot(&mut self, next: Arc<Snapshot>, cx: &mut Context<Self>) {
        let first_load = !self.snapshot.loaded && next.loaded;
        let new_flow = next
            .flows
            .iter()
            .map(|c| c.flow.id)
            .find(|id| self.snapshot.flow(*id).is_none());
        self.snapshot = next;
        if let Some(id) = new_flow.filter(|_| self.pending_open) {
            self.pending_open = false;
            self.select(id, cx);
        }
        if let Some((flow, before)) = self.pending_conversation
            && let Some(view) = self.snapshot.flow(flow)
            && view.conversations.len() > before
        {
            self.pending_conversation = None;
            if let Some(newest) = view.conversations.last() {
                self.tabs.insert(flow, Tab::Conversation(*newest));
            }
        }
        if first_load {
            // Notifications from before this window opened are not shown again.
            self.shown_notifications = self.snapshot.notifications.last().map_or(0, |n| n.id);
        }
        for n in self
            .snapshot
            .notifications
            .iter()
            .filter(|n| n.id > self.shown_notifications)
        {
            cx.show_system_notification(SystemNotification {
                tag: format!("conversation-{}", n.flow.0).into(),
                title: n.title.clone().into(),
                body: n.body.clone().into(),
                actions: Vec::new(),
            });
        }
        self.shown_notifications = self
            .snapshot
            .notifications
            .last()
            .map_or(self.shown_notifications, |n| n.id);
        self.show_flash(cx);
        if let Screen::Flow(id) = self.screen
            && self.snapshot.flow(id).is_none()
        {
            self.screen = Screen::Nothing;
        }
        cx.notify();
    }

    /// Shows a flow on its Flow tab (NAV-7, FLOW-4).
    pub(crate) fn select(&mut self, id: FlowId, cx: &mut Context<Self>) {
        self.screen = Screen::Flow(id);
        self.tabs.remove(&id);
        self.close_detail();
        self.open_picker = None;
        self.rename = None;
        self.row_menu = None;
        self.core.send(Intent::View(Some(id)));
        cx.notify();
    }

    /// The conversation shown for the selected flow; `None` when
    /// the Flow tab shows, which is where a flow opens (FLOW-4).
    pub(crate) fn current_conversation(&self) -> Option<ConversationId> {
        let Screen::Flow(id) = self.screen else {
            return None;
        };
        let flow = self.snapshot.flow(id)?;
        match self.tabs.get(&id) {
            Some(Tab::Conversation(s)) if flow.conversations.contains(s) => Some(*s),
            _ => None,
        }
    }

    /// Closes the issue or pull request drawer, if open.
    pub(crate) fn close_detail(&self) {
        if self.snapshot.detail.is_some() {
            self.core.send(Intent::ShowDetail(None));
        }
    }

    pub(crate) fn show_new_flow(&mut self, cx: &mut Context<Self>) {
        self.close_detail();
        self.screen = Screen::NewFlow;
        self.open_picker = None;
        if self.form.project.is_none() {
            self.form.project = self.snapshot.projects.first().map(|p| p.id);
        }
        self.default_agent();
        self.core.send(Intent::View(None));
        cx.notify();
    }

    /// Opens the new-flow form on `project` (when known). For an
    /// issue, the flow is linked to it and the message starts with
    /// `owner/name#12` and a blank line, the caret on it.
    pub(crate) fn open_new_flow(
        &mut self,
        project: Option<ProjectId>,
        issue: Option<leon_domain::GhRef>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if project.is_some() {
            self.form.project = project;
            self.form.settings.agent = None;
        }
        self.show_new_flow(cx);
        if let Some(issue) = issue {
            let message = format!("{}\n\n", issue.text());
            crate::message_input::write_in(&self.form_text, message, window, cx);
            self.form.issue = Some(issue);
        }
        self.form_text.focus_handle(cx).focus(window, cx);
    }

    pub(crate) fn add_project(&mut self, cx: &mut Context<Self>) {
        let paths = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Add project".into()),
        });
        let core = self.core.clone();
        cx.spawn(async move |_, _| {
            if let Ok(Ok(Some(paths))) = paths.await {
                for path in paths {
                    core.send(Intent::AddProject(path));
                }
            }
        })
        .detach();
    }

    /// The transcript list of a conversation, kept in sync with its item count.
    pub(crate) fn list_for(
        &mut self,
        conversation: ConversationId,
        count: usize,
        streaming: bool,
    ) -> ListState {
        let entry = self.lists.entry(conversation).or_insert_with(|| {
            let state = ListState::new(0, ListAlignment::Bottom, px(400.));
            state.set_follow_mode(FollowMode::Tail);
            (state, 0)
        });
        let (state, known) = entry;
        if count > *known {
            state.splice(*known..*known, count - *known);
        } else if count < *known {
            state.reset(count);
        }
        if streaming && count > 0 {
            state.remeasure_items(count - 1..count);
        }
        *known = count;
        state.clone()
    }

    pub(crate) fn backend_options(&self, backend: BackendKind) -> leon_core::ports::BackendOptions {
        self.snapshot
            .options
            .get(&backend)
            .cloned()
            .unwrap_or_default()
    }
}

impl Render for Workspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if let Some(started) = self.started.take() {
            window.on_next_frame(move |_window, _cx| {
                tracing::info!(startup_ms = started.elapsed().as_millis(), "first frame");
            });
        }
        // An answer is read on its agent's tab, not on the Flow (NAV-4).
        let reading = self.current_conversation();
        if reading != self.reading {
            self.reading = reading;
            self.core.send(Intent::Read(reading));
        }
        let main: AnyElement = match self.screen {
            Screen::Nothing => crate::widgets::empty("Select a flow, or start a new one."),
            Screen::Flow(id) => self.render_flow_pane(id, window, cx),
            Screen::NewFlow => self.render_new_flow(None, window, cx),
            Screen::GitHub(list) => self.render_github_list(list, cx),
            Screen::Doctor => self.render_doctor(cx),
            Screen::Tools => self.render_tools(cx),
            Screen::Settings => self.render_settings(cx),
        };
        let root = self.shortcut_handlers(div(), cx);
        root.size_full()
            .relative()
            .flex()
            .flex_row()
            .bg(theme::window())
            .text_color(theme::fg())
            .text_size(px(13.))
            .on_mouse_move(cx.listener(|this, e, window, cx| this.resize_panes(e, window, cx)))
            .on_mouse_up(
                gpui::MouseButton::Left,
                cx.listener(|this, _, window, cx| {
                    this.panes.resizing = None;
                    this.finish_selection(window, cx);
                }),
            )
            .child(self.render_left(window, cx))
            .child(Self::pane_handle(Side::Left, cx))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .flex()
                    .flex_col()
                    .child(main),
            )
            .when(self.panes.right_open, |d| {
                d.child(Self::pane_handle(Side::Right, cx))
                    .child(self.render_right(window, cx))
            })
            .children(self.render_drawer(window, cx))
            .children(self.render_detail(cx))
            .children(self.render_flash())
    }
}

/// Text of a one-line input.
pub(crate) fn value_of(input: &Entity<InputState>, cx: &App) -> String {
    input.read(cx).value().to_string()
}

/// Text of a multi-line input.
pub(crate) fn text_of(input: &Entity<TextareaState>, cx: &App) -> String {
    input.read(cx).value().to_string()
}
