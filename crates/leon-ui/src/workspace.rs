//! The window's root view: owns the UI-only state (what is selected, which
//! picker is open) and renders the three panes from the core's snapshot.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui::{
    AnyElement, App, Context, Entity, FollowMode, IntoElement, ListAlignment, ListState, Render,
    SharedString, Subscription, SystemNotification, Task, Window, div, prelude::*, px,
};
use gpui_component::input::{InputEvent, InputState, TextareaState};
use leon_core::{CoreHandle, Intent, Snapshot};
use leon_domain::{AgentSessionId, BackendKind, ConversationId, ProjectId, SessionSettings};

use crate::flow::FlowState;
use crate::github::GhList;
use crate::right::RightState;
use crate::theme;

/// What the main pane shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Screen {
    Nothing,
    Conversation(ConversationId),
    NewConversation,
    /// My issues or my pull requests (GH-2, GH-3).
    GitHub(GhList),
    /// What works and what needs setting up.
    Doctor,
}

/// The tab shown for a conversation (FLOW-3): the flow, or one agent session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Tab {
    Flow,
    Session(AgentSessionId),
}

/// Choices in the new-conversation form.
#[derive(Debug, Clone)]
pub(crate) struct NewForm {
    pub project: Option<ProjectId>,
    pub settings: SessionSettings,
}

/// Root view of the window.
pub struct Workspace {
    pub(crate) core: CoreHandle,
    pub(crate) snapshot: Arc<Snapshot>,
    pub(crate) screen: Screen,
    pub(crate) tabs: HashMap<ConversationId, Tab>,
    pub(crate) composer: Entity<TextareaState>,
    pub(crate) form_text: Entity<TextareaState>,
    pub(crate) search: Entity<InputState>,
    pub(crate) rename: Option<(ConversationId, Entity<InputState>)>,
    pub(crate) form: NewForm,
    pub(crate) open_picker: Option<SharedString>,
    pub(crate) show_archived: bool,
    pub(crate) confirm_delete: Option<ConversationId>,
    /// Tool calls and long messages the user unfolded.
    pub(crate) expanded: HashSet<(AgentSessionId, usize)>,
    /// Long pastes waiting in the message box (CONV-12).
    pub(crate) pasted: Vec<String>,
    /// The user's message being edited: sending goes back to before it
    /// (CONV-7, CONV-8).
    pub(crate) rewind: Option<(AgentSessionId, usize)>,
    pub(crate) lists: HashMap<AgentSessionId, (ListState, usize)>,
    pub(crate) right: RightState,
    pub(crate) flow: FlowState,
    /// The flash message shown, by id (an action's result).
    flash: Option<(u64, String)>,
    /// Open the next new conversation when it shows up.
    pub(crate) pending_open: bool,
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
                .placeholder("Message the agent")
        });
        let form_text = cx.new(|cx| {
            TextareaState::new(window, cx)
                .auto_grow(3, 12)
                .submit_on_enter(true)
                .placeholder("What should the agent do?")
        });
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search conversations"));
        let (right, right_subscriptions) = RightState::new(window, cx);
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
                        this.start_conversation(window, cx);
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
        let snapshot = core.snapshot();
        let defaults = snapshot.settings.defaults.clone();
        Self {
            core: core.clone(),
            snapshot,
            screen: Screen::Nothing,
            tabs: HashMap::new(),
            composer,
            form_text,
            search,
            rename: None,
            form: NewForm {
                project: None,
                settings: SessionSettings {
                    backend: defaults.backend,
                    agent: None,
                    model: defaults.model,
                    effort: defaults.effort,
                    permission: defaults.permission,
                },
            },
            open_picker: None,
            show_archived: false,
            confirm_delete: None,
            expanded: HashSet::new(),
            pasted: Vec::new(),
            rewind: None,
            lists: HashMap::new(),
            right,
            flow: FlowState::default(),
            flash: None,
            pending_open: false,
            shown_notifications: 0,
            started: Some(started),
            _updates: updates,
            _subscriptions: subscriptions,
        }
    }

    fn on_snapshot(&mut self, next: Arc<Snapshot>, cx: &mut Context<Self>) {
        let first_load = !self.snapshot.loaded && next.loaded;
        let new_conversation = next
            .conversations
            .iter()
            .map(|c| c.conversation.id)
            .find(|id| self.snapshot.conversation(*id).is_none());
        self.snapshot = next;
        if let Some(id) = new_conversation.filter(|_| self.pending_open) {
            self.pending_open = false;
            self.select(id, cx);
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
                tag: format!("conversation-{}", n.conversation.0).into(),
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
        if let Screen::Conversation(id) = self.screen
            && self.snapshot.conversation(id).is_none()
        {
            self.screen = Screen::Nothing;
        }
        cx.notify();
    }

    /// Shows the core's latest flash message for a few seconds.
    fn show_flash(&mut self, cx: &mut Context<Self>) {
        let Some((id, text)) = self.snapshot.flash.clone() else {
            return;
        };
        if self.flash.as_ref().is_some_and(|(shown, _)| *shown >= id) {
            return;
        }
        self.flash = Some((id, text));
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_secs(4)).await;
            this.update(cx, |this, cx| {
                if this.flash.as_ref().is_some_and(|(shown, _)| *shown == id) {
                    this.flash = None;
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    /// Shows a conversation (NAV-7).
    pub(crate) fn select(&mut self, id: ConversationId, cx: &mut Context<Self>) {
        self.screen = Screen::Conversation(id);
        self.open_picker = None;
        self.confirm_delete = None;
        self.rename = None;
        self.core.send(Intent::View(Some(id)));
        cx.notify();
    }

    /// The agent session shown for the selected conversation, if any.
    pub(crate) fn current_session(&self) -> Option<AgentSessionId> {
        let Screen::Conversation(id) = self.screen else {
            return None;
        };
        let conversation = self.snapshot.conversation(id)?;
        match self.tabs.get(&id) {
            Some(Tab::Session(s)) if conversation.sessions.contains(s) => Some(*s),
            Some(Tab::Flow) => None,
            _ => conversation.sessions.first().copied(),
        }
    }

    pub(crate) fn show_new_conversation(&mut self, cx: &mut Context<Self>) {
        self.screen = Screen::NewConversation;
        self.open_picker = None;
        if self.form.project.is_none() {
            self.form.project = self.snapshot.projects.first().map(|p| p.id);
        }
        self.core.send(Intent::View(None));
        cx.notify();
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

    /// The transcript list of a session, kept in sync with its item count.
    pub(crate) fn list_for(
        &mut self,
        session: AgentSessionId,
        count: usize,
        streaming: bool,
    ) -> ListState {
        let entry = self.lists.entry(session).or_insert_with(|| {
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
        let main: AnyElement = match self.screen {
            Screen::Nothing => crate::widgets::empty("Select a conversation, or start a new one."),
            Screen::Conversation(id) => self.render_conversation(id, window, cx),
            Screen::NewConversation => self.render_new_conversation(window, cx),
            Screen::GitHub(list) => self.render_github_list(list, cx),
            Screen::Doctor => self.render_doctor(cx),
        };
        let flash = self.flash.as_ref().map(|(_, text)| {
            div()
                .absolute()
                .bottom_4()
                .left_0()
                .right_0()
                .flex()
                .justify_center()
                .child(
                    div()
                        .px_3()
                        .py_2()
                        .rounded_lg()
                        .border_1()
                        .border_color(theme::border_strong())
                        .bg(theme::surface())
                        .shadow_lg()
                        .text_size(px(12.5))
                        .child(text.clone()),
                )
        });
        div()
            .size_full()
            .relative()
            .flex()
            .flex_row()
            .bg(theme::window())
            .text_color(theme::fg())
            .text_size(px(13.))
            .child(self.render_left(window, cx))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .flex()
                    .flex_col()
                    .child(main),
            )
            .child(self.render_right(window, cx))
            .children(flash)
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
