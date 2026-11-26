//! The ACP client state machine, free of I/O: messages in, events and
//! messages out.

use std::collections::HashMap;

use leon_core::ports::{BackendCommand, BackendEvent, Prompt, SessionSpec};
use serde_json::{Value, json};

use super::permission::Permission;
use super::setup::{self, Capabilities, PROTOCOL_VERSION, Start};
use super::turn::Turn;

/// What a request Leon sent is waiting for.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Pending {
    Initialize,
    /// `session/new`, or `session/load` / `session/resume` of that id.
    Session(Option<String>),
    /// `session/set_config_option`.
    Selector,
    Prompt,
}

/// One conversation over ACP.
#[derive(Debug)]
pub(crate) struct Session {
    /// The CLI's name, for messages.
    label: &'static str,
    spec: SessionSpec,
    /// `(category, value)` to set through the agent's selectors.
    selectors: Vec<(&'static str, String)>,
    next_id: i64,
    pending: HashMap<i64, Pending>,
    /// The ACP session id, once the agent gave or accepted it.
    id: Option<String>,
    /// Selector requests not answered yet; prompts wait for them.
    selecting: usize,
    /// `session/load` replays the history: not shown again.
    loading: bool,
    queued: Vec<Prompt>,
    /// The agent's instructions, sent with the first prompt when the CLI
    /// takes no system prompt.
    preamble: Option<String>,
    prompts: usize,
    turn: Turn,
    permissions: HashMap<String, Permission>,
}

impl Session {
    /// A new session and the messages to send first.
    pub(crate) fn new(spec: &SessionSpec, label: &'static str) -> (Self, Vec<Value>) {
        let s = &spec.settings;
        let selectors = [
            ("model", s.model.as_deref()),
            ("thought_level", s.effort.as_deref()),
        ]
        .into_iter()
        .filter_map(|(category, value)| Some((category, value?.to_owned())))
        .collect();
        let mut session = Self {
            label,
            spec: spec.clone(),
            selectors,
            next_id: -1,
            pending: HashMap::new(),
            id: None,
            selecting: 0,
            loading: false,
            queued: Vec::new(),
            preamble: None,
            prompts: 0,
            turn: Turn::default(),
            permissions: HashMap::new(),
        };
        let init = session.request(
            Pending::Initialize,
            "initialize",
            &setup::initialize_params(),
        );
        (session, vec![init])
    }

    /// The session found again under its holder (I24): set up, in session
    /// `spec.resume`, maybe in a turn.
    pub(crate) fn attached(spec: &SessionSpec, label: &'static str) -> Self {
        let (mut session, _) = Self::new(spec, label);
        session.pending.clear();
        session.id.clone_from(&spec.resume);
        session.selectors.clear();
        // Far from the ids the CLI may still answer from before.
        session.next_id = 1_000_000;
        session.prompts = 1;
        session
    }

    fn request(&mut self, kind: Pending, method: &str, params: &Value) -> Value {
        self.next_id += 1;
        self.pending.insert(self.next_id, kind);
        json!({ "jsonrpc": "2.0", "id": self.next_id, "method": method, "params": params })
    }

    /// `session/prompt`: the text, after each image as an ACP `image` block,
    /// base64 and its MIME type (CONV-18; ACP v1 schema, 2026-11-24).
    fn prompt(&mut self, session: &str, prompt: Prompt) -> Value {
        let text = match self.preamble.take() {
            Some(instructions) => format!("{instructions}\n\n{}", prompt.text),
            None => prompt.text,
        };
        self.prompts += 1;
        let mut blocks: Vec<Value> = prompt
            .images
            .iter()
            .filter_map(|path| crate::images::encoded(path))
            .map(|(mime, data)| json!({ "type": "image", "mimeType": mime, "data": data }))
            .collect();
        blocks.push(json!({ "type": "text", "text": text }));
        let params = json!({ "sessionId": session, "prompt": blocks });
        self.request(Pending::Prompt, "session/prompt", &params)
    }

    /// Queued prompts, once the session is set up.
    fn send_queued(&mut self) -> Vec<Value> {
        match self.id.clone() {
            Some(session) if self.selecting == 0 => std::mem::take(&mut self.queued)
                .into_iter()
                .map(|text| self.prompt(&session, text))
                .collect(),
            _ => Vec::new(),
        }
    }

    /// A command from the core; returns the messages to send.
    pub(crate) fn command(&mut self, command: BackendCommand) -> Vec<Value> {
        match command {
            BackendCommand::Send(prompt) => {
                self.queued.push(prompt);
                self.send_queued()
            }
            BackendCommand::Interrupt => match &self.id {
                Some(session) if self.prompts > 0 => {
                    let cancel = json!({
                        "jsonrpc": "2.0",
                        "method": "session/cancel",
                        "params": { "sessionId": session },
                    });
                    let mut out = vec![cancel];
                    out.extend(self.permissions.drain().map(|(_, p)| p.cancel()));
                    out
                }
                _ => Vec::new(),
            },
            BackendCommand::Answer { id, answer } => self
                .permissions
                .remove(&id)
                .map(|p| p.answer(&answer))
                .into_iter()
                .collect(),
        }
    }

    /// A message from the agent; returns events and messages to send back.
    pub(crate) fn handle(&mut self, msg: &Value) -> (Vec<BackendEvent>, Vec<Value>) {
        let method = msg.get("method").and_then(Value::as_str);
        match (msg.get("id"), method) {
            (Some(id), None) => self.response(id, msg),
            (Some(id), Some(method)) => self.agent_request(id, method, &msg["params"]),
            (None, Some("session/update")) if !self.loading => {
                (self.turn.update(&msg["params"]["update"]), Vec::new())
            }
            (None, _) => (Vec::new(), Vec::new()),
        }
    }

    fn response(&mut self, id: &Value, msg: &Value) -> (Vec<BackendEvent>, Vec<Value>) {
        let Some(kind) = id.as_i64().and_then(|id| self.pending.remove(&id)) else {
            // A prompt sent before Leon came back (I24): its end still ends
            // the turn.
            if msg["result"].get("stopReason").is_some() {
                self.prompts = self.prompts.saturating_sub(1);
                return (self.turn.end(&msg["result"]), Vec::new());
            }
            return (Vec::new(), Vec::new());
        };
        if let Some(error) = msg.get("error") {
            let text = error["message"]
                .as_str()
                .unwrap_or("the agent refused a request");
            return match kind {
                Pending::Selector => {
                    let (mut events, out) = self.selected();
                    events.push(BackendEvent::Notice(format!(
                        "{} refused a setting: {text}",
                        self.label
                    )));
                    (events, out)
                }
                Pending::Prompt => {
                    // The turn is over, as when Claude Code or Codex report
                    // a failed turn.
                    self.prompts = self.prompts.saturating_sub(1);
                    let mut events = self.turn.close();
                    events.push(BackendEvent::Failed(text.to_owned()));
                    events.push(BackendEvent::TurnEnded {
                        stopped: false,
                        usage: None,
                    });
                    (events, Vec::new())
                }
                Pending::Initialize | Pending::Session(_) => {
                    self.loading = false;
                    (vec![BackendEvent::Failed(text.to_owned())], Vec::new())
                }
            };
        }
        let result = &msg["result"];
        match kind {
            Pending::Initialize => self.initialized(result),
            Pending::Session(resumed) => self.started(resumed, result),
            Pending::Selector => self.selected(),
            Pending::Prompt => {
                self.prompts = self.prompts.saturating_sub(1);
                (self.turn.end(result), Vec::new())
            }
        }
    }

    fn initialized(&mut self, result: &Value) -> (Vec<BackendEvent>, Vec<Value>) {
        let version = result["protocolVersion"].as_u64();
        if version != Some(PROTOCOL_VERSION) {
            let text = format!(
                "{} speaks ACP version {}; Leon speaks version {PROTOCOL_VERSION}",
                self.label, result["protocolVersion"]
            );
            return (vec![BackendEvent::Failed(text)], Vec::new());
        }
        let caps = Capabilities::read(result);
        let start = Start::choose(self.spec.resume.as_deref(), caps);
        let mut events = Vec::new();
        if start == Start::New {
            if self.spec.resume.is_some() {
                events.push(BackendEvent::Notice(format!(
                    "{} cannot resume its session: a new one started, without the earlier messages.",
                    self.label
                )));
            }
            if !caps.system_prompt {
                self.preamble = setup::told(&self.spec);
            }
        }
        self.loading = matches!(start, Start::Load(_));
        let (method, params) = setup::session_request(&self.spec, &start, caps);
        let resumed = start.resumed().map(str::to_owned);
        (
            events,
            vec![self.request(Pending::Session(resumed), method, &params)],
        )
    }

    fn started(
        &mut self,
        resumed: Option<String>,
        result: &Value,
    ) -> (Vec<BackendEvent>, Vec<Value>) {
        self.loading = false;
        let Some(id) = resumed.or_else(|| result["sessionId"].as_str().map(str::to_owned)) else {
            let text = format!("{} did not return a session", self.label);
            return (vec![BackendEvent::Failed(text)], Vec::new());
        };
        self.id = Some(id.clone());
        let (requests, missing) = setup::selector_requests(&id, result, &self.selectors);
        let mut out: Vec<Value> = requests
            .into_iter()
            .map(|(method, params)| self.request(Pending::Selector, method, &params))
            .collect();
        self.selecting = out.len();
        out.extend(self.send_queued());
        let mut events = vec![BackendEvent::Ready { cli_session: id }];
        // Its models, for the pickers (I31).
        let models = crate::models::acp_models(result);
        if !models.is_empty() {
            events.push(BackendEvent::Models(models));
        }
        events.extend(missing.into_iter().map(|setting| {
            BackendEvent::Notice(format!(
                "{} does not offer {setting}: it uses its default.",
                self.label
            ))
        }));
        (events, out)
    }

    fn selected(&mut self) -> (Vec<BackendEvent>, Vec<Value>) {
        self.selecting = self.selecting.saturating_sub(1);
        (Vec::new(), self.send_queued())
    }

    /// A request from the agent. Only permission requests are supported:
    /// Leon advertises no file system, terminal or elicitation.
    fn agent_request(
        &mut self,
        id: &Value,
        method: &str,
        params: &Value,
    ) -> (Vec<BackendEvent>, Vec<Value>) {
        if method != "session/request_permission" {
            tracing::debug!(cli = self.label, method, "unsupported ACP request");
            let error = json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": { "code": -32601, "message": "Method not found" },
            });
            return (Vec::new(), vec![error]);
        }
        let known = params["toolCall"]["toolCallId"]
            .as_str()
            .and_then(|tool| self.turn.tool(tool));
        let (question, permission) = Permission::ask(id, params, known);
        if let BackendEvent::Question { id, .. } = &question {
            self.permissions.insert(id.clone(), permission);
        }
        (vec![question], Vec::new())
    }
}
