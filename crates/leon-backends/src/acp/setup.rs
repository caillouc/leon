//! What Leon sends to set an ACP session up: `initialize`, then
//! `session/new`, `session/load` or `session/resume`, then the agent's own
//! selectors for model, effort and mode.

use leon_core::ports::SessionSpec;
use serde_json::{Value, json};

/// The ACP major version Leon speaks.
pub(super) const PROTOCOL_VERSION: u64 = 1;

/// `initialize` params: no file system, no terminal; the agent does both
/// itself.
pub(super) fn initialize_params() -> Value {
    json!({
        "protocolVersion": PROTOCOL_VERSION,
        "clientCapabilities": {
            "fs": { "readTextFile": false, "writeTextFile": false },
            "terminal": false,
        },
        "clientInfo": { "name": "leon", "version": env!("CARGO_PKG_VERSION") },
    })
}

/// What the agent said it can do in its `initialize` response.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct Capabilities {
    /// `session/load`: history replayed, then the response.
    pub load: bool,
    /// `session/resume`: no replay.
    pub resume: bool,
    /// `systemPrompt` in `append` mode on session requests.
    pub system_prompt: bool,
}

impl Capabilities {
    pub(super) fn read(result: &Value) -> Self {
        let agent = &result["agentCapabilities"];
        let session = &agent["sessionCapabilities"];
        Self {
            load: agent["loadSession"] == true,
            resume: session["resume"].is_object(),
            system_prompt: session["systemPrompt"].is_object(),
        }
    }
}

/// How the agent session starts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Start {
    New,
    Load(String),
    Resume(String),
}

impl Start {
    /// `session/resume` when the agent has it (no replay), else
    /// `session/load`, else a new session.
    pub(super) fn choose(resume: Option<&str>, caps: Capabilities) -> Self {
        match resume {
            Some(id) if caps.resume => Self::Resume(id.to_owned()),
            Some(id) if caps.load => Self::Load(id.to_owned()),
            Some(_) | None => Self::New,
        }
    }

    /// The session id it continues, if any.
    pub(super) fn resumed(&self) -> Option<&str> {
        match self {
            Self::New => None,
            Self::Load(id) | Self::Resume(id) => Some(id),
        }
    }
}

/// The session request and its params.
/// What the agent is told, in the system prompt or before its first
/// message: Leon's rules for its tools (CTX-3), then the agent's own
/// instructions. `~/.agents/AGENTS.md` goes through Copilot's own setting.
pub(super) fn told(spec: &SessionSpec) -> Option<String> {
    crate::told::told([
        spec.app_instructions,
        spec.agent.as_ref().map(|a| a.instructions.as_str()),
    ])
}

pub(super) fn session_request(
    spec: &SessionSpec,
    start: &Start,
    caps: Capabilities,
) -> (&'static str, Value) {
    let mut params = json!({
        "cwd": spec.working_dir.to_string_lossy(),
        // Copilot CLI drops stdio servers sent here (1.0.113 log: "Rejecting
        // non-http/sse MCP server"): they go in its start flags instead.
        "mcpServers": [],
    });
    if caps.system_prompt
        && let Some(text) = told(spec)
    {
        params["systemPrompt"] = json!({
            "mode": "append",
            "content": [{ "type": "text", "text": text }],
        });
    }
    let method = match start {
        Start::New => "session/new",
        Start::Load(id) => {
            params["sessionId"] = json!(id);
            "session/load"
        }
        Start::Resume(id) => {
            params["sessionId"] = json!(id);
            "session/resume"
        }
    };
    (method, params)
}

/// A request to send: method and params.
pub(super) type Request = (&'static str, Value);

/// Requests that move the agent's selectors to what Leon asks for:
/// `(category, value)` pairs, e.g. `("model", "gpt-6-luna")`, through the
/// session config options. A value matches on its id or its name; a selector already on it is left alone. Also returns, for
/// the user, each value the agent does not offer.
pub(super) fn selector_requests(
    session: &str,
    result: &Value,
    wanted: &[(&'static str, String)],
) -> (Vec<Request>, Vec<String>) {
    let mut requests = Vec::new();
    let mut missing = Vec::new();
    for (category, value) in wanted {
        let request = match result["configOptions"].as_array() {
            Some(options) => config_request(session, options, category, value),
            None => None,
        };
        match request {
            Some((request, true)) => requests.push(request),
            Some((_, false)) => {}
            None => missing.push(format!("{category} {value}")),
        }
    }
    (requests, missing)
}

/// The request setting the config option of `category` to `value`, and
/// whether it changes anything; `None` when the agent offers no such value.
fn config_request(
    session: &str,
    options: &[Value],
    category: &str,
    value: &str,
) -> Option<(Request, bool)> {
    let option = options
        .iter()
        .find(|o| o["category"] == category && o["type"] == "select")?;
    let chosen = select_value(&option["options"], value)?;
    let params = json!({ "sessionId": session, "configId": option["id"], "value": chosen });
    Some((
        ("session/set_config_option", params),
        *chosen != option["currentValue"],
    ))
}

/// The id of the option matching `wanted`, in a flat or grouped list.
fn select_value<'a>(options: &'a Value, wanted: &str) -> Option<&'a Value> {
    options
        .as_array()?
        .iter()
        .flat_map(|o| match o["options"].as_array() {
            Some(group) => group.as_slice(),
            None => std::slice::from_ref(o),
        })
        .find(|o| same(&o["value"], wanted) || same(&o["name"], wanted))
        .map(|o| &o["value"])
}

fn same(value: &Value, wanted: &str) -> bool {
    value
        .as_str()
        .is_some_and(|s| s.eq_ignore_ascii_case(wanted))
}
