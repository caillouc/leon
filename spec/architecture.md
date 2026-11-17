# Architecture

How the code is organised. Owned by agents; must follow
[decisions.md](decisions.md). Follows D1–D5 and P1–P15
(accepted 2026-11-17).

## Processes

- One app process: UI thread (GPUI, P1), core on tokio (P8), a local MCP server
  for app tools (P10).
- One CLI child process per agent session while a turn runs or input is
  awaited; stopped after an idle timeout, resumed by the CLI's session id. A CLI
  crash sets that agent session to `error`; it never takes the app down.
- `git` spawned per query (P6); GitHub over HTTPS (P7).

## Crates and layers

Ports and adapters: dependencies point inward, to the core and the domain.
`cargo xtask layers` checks it in CI (table in `xtask/src/layers.rs`).

| Ring | Crate | Does | May depend on | Status |
|---|---|---|---|---|
| 1 | `leon-domain` | Pure types and rules: ids, projects, settings; later conversation state, events, status/unread rules, PR badge, fold matching | nothing internal | M0 |
| 2 | `leon-core` | Owns state, publishes snapshots, defines the ports (traits); later starts/resumes conversations and routes events | domain | M0 |
| 3 | adapters: `leon-config` (TOML → settings), `leon-store` (SQLite: `Storage`), `leon-backends` (Claude Code stream-json, Codex app-server: `Backends`), `leon-catalog` (agent files: `Catalog`); later `leon-git`, `leon-github`, `leon-app-tools` | Implement one port each against the outside world | domain, core; never another adapter | M0–M1 |
| 3 | `leon-ui` | Views, input → intents. The only crate that uses GPUI | domain, core | M0 |
| 4 | `leon` (bin) | Builds the adapters, starts the core, starts the UI | any | M0 |
| — | `xtask` | `cargo xtask ci`, `cargo xtask layers` | no Leon crate | M0 |

A new crate goes into both this table and `xtask/src/layers.rs`, or CI fails.

## Data flow

```
UI intent ─▶ core command ─▶ Backend port ─▶ CLI
CLI output ─▶ adapter ─▶ domain Event ─▶ apply(state, event) ─▶ store ─▶ UI notified
```

- The core is the only writer of state; the UI reads snapshots.
- `apply(SessionState, Event) -> SessionState` is pure (in `leon-domain`);
  status, unread and usage derive from events. Replaying stored events rebuilds
  an agent session.
- A conversation's status and unread are derived from its agent sessions
  (running if any runs, `needs_input` if any waits, and so on).

## Agent sessions (D5, P15)

```
Conversation
 ├─ AgentSession  planner   (started by the user)
 ├─ AgentSession  coder     (started by planner)
 ├─ AgentSession  reviewer  (started by planner)
 └─ AgentCall[]   from, to, prompt, answer, started_at, answered_at
```

1. The planner calls `start_agent(agent, prompt, working_directory?)` on the
   app's MCP server (`leon-app-tools`), which knows which session is calling.
2. The core creates the coder's agent session in the same conversation, starts
   its CLI, sends the prompt, records an `AgentCall`, and returns its id at once.
3. When the coder's turn ends, the core sends the coder's final answer as a new
   message to the planner's session and fills `AgentCall.answer`.
4. `message_agent(session, text)` follows the same path for follow-ups.
5. The flow view (FLOW-4) is drawn from the `AgentCall` rows only.

A message the user types in the coder's session is answered to the user, not
forwarded to the planner (proposal in features.md).

## Events (sketch, shaped like ACP session updates — P2)

`SessionStarted` · `UserMessage` · `AgentTextDelta` / `AgentMessage` ·
`ThinkingDelta` · `ToolCall` / `ToolCallUpdate` · `PermissionRequest` ·
`Question` · `Usage` · `WorkingDirChanged` · `TurnEnded` · `BackendError`.
Persisted (P5), so versioned with migration tests.

## CLIs (P2)

`local` = checked on the installed CLI's `--help` on 2026-11-17; others from
official docs the same day, re-checked by contract tests before each adapter
ships.

| | Claude Code | Codex | Copilot CLI | Antigravity (`agy`) |
|---|---|---|---|---|
| Checked on | 2.1.288 local | 0.160.0 local | docs | docs |
| Transport | `-p --input-format stream-json --output-format stream-json --include-partial-messages` (local) | `codex app-server` JSON-RPC, marked experimental; schema from `generate-json-schema` (local) | `--acp` (preview) | `agy acp` |
| Agent | `--agent`, `--agents <json>` (local) | No `--agent` flag (local) → agent's instructions in `thread/start.developerInstructions` (local schema) | `--agent` | `--agent` ⚠️ |
| Model / effort | `--model`, `--effort` (local) | `turn/start.model`, `turn/start.effort` — per turn, so switchable mid-conversation (local schema) | `--model`, `--effort` | `--model`, `--effort` |
| Stop (CONV-6) | interrupt over stream-json ⚠️ not verified | `turn/interrupt` (local schema) | ACP `session/cancel` | ACP `session/cancel` |
| Go back (CONV-7) | no flag in `--help` (local) ⚠️ — fallback: new session carrying the messages up to that point | `thread/revert { beforeTurnId }` (local schema) | ⚠️ | ⚠️ |
| Subagent counts (CONV-13) | `parent_tool_use_id`; `--forward-subagent-text` (local) | `CollabAgent*` items (local schema) | ⚠️ | `subagent_info` ⚠️ |
| Suggested replies (CONV-10) | `--prompt-suggestions` → `prompt_suggestion` message (local) | ⚠️ | ⚠️ | ⚠️ |
| Title (NAV-9) | ⚠️ | `thread/name/set`, `thread/name/updated` (local schema) | ⚠️ | ⚠️ |
| Permission | `--permission-mode acceptEdits\|auto\|bypassPermissions\|manual\|dontAsk\|plan` (local) | `-a`, `-s`, `--dangerously-bypass-approvals-and-sandbox` (local) | ACP modes | ACP modes |
| MCP list | `--mcp-config` (local) | `-c mcp_servers.<name>=…` (local) | `--additional-mcp-config`, ACP `mcpServers` | ACP `mcpServers` |
| Resume | `--resume`, `--fork-session` (local) | `thread/resume`, `thread/fork` | `--resume`, ACP `session/load` | ACP `session/resume` |
| Usage | `result.total_cost_usd`, `rate_limit_event` | `thread/tokenUsage/updated`, `account/rateLimits/read` | `/usage` | `usage` object |

Rules for every adapter:

- Always pass model, effort, permission and agent explicitly. Claude Code's docs
  announce `-p` defaulting to manual mode (v2.1.302) and `--bare` becoming the
  default ⚠️ not verified locally.
- Declare the supported CLI version range; check `--version` at startup.
- Pickers show what the CLI itself offers.

## Agents, skills, MCP (P3)

Where each CLI looks (docs, 2026-11-17):

| | Claude Code | Codex | Copilot CLI | Gemini / agy |
|---|---|---|---|---|
| Instructions | `CLAUDE.md` (`AGENTS.md` only if no CLAUDE.md) | `AGENTS.md` | `AGENTS.md`, `CLAUDE.md` | `GEMINI.md`, `AGENTS.md` via setting |
| Skills | `~/.claude/skills`, plugins | `~/.agents/skills` | `~/.agents/skills`, `.github/skills`, `.claude/skills` | `~/.agents/skills` |
| Agents | `.claude/agents/*.md` | TOML in `.codex/agents/` | `*.agent.md` | `.gemini/agents/*.md`; agy: plugins |
| Plugins | `.claude-plugin/` | reads Claude manifests | reads `.claude-plugin/` | agy imports Claude plugins |

`leon-catalog` reads the user's source folders (agents, skills, `.mcp.json`) for
the agent picker and the MCP list; each adapter translates the chosen agent and
MCP list for its CLI.

## git (P6)

- Uncommitted: `git status --porcelain=v2 -z` + per-file diff against `HEAD`.
- Branch diff: `git diff <merge-base(base, HEAD)>..HEAD`.
- Refresh on file-watcher events (debounced) and at turn end; never polled.
- Diffs computed per file, lazily, off the UI thread; rendered virtualised.

## GitHub (P7)

- Token from `gh auth token`, else `GITHUB_TOKEN`; never stored.
- One GraphQL query refreshes the PRs of all visible conversations; paused when
  the window is hidden; conditional requests and back-off on rate limits.
- Copilot review request: API route ⚠️ not verified; fallback
  `gh pr edit --add-reviewer @copilot`.

## Speed targets (QUAL-1, QUAL-2)

Engineering targets for "as fast as Zed", app process only (CLIs excluded).
Agents measure against them; they are not features.

| What | Target |
|---|---|
| Cold start → interactive | < 300 ms |
| Keystroke → painted | < 16 ms |
| Switch to a 1,000-message conversation | < 50 ms |
| 10 conversations streaming at once | 60 fps |
| Open a 10,000-line diff | first screen < 200 ms, scroll at 60 fps |
| Idle memory, 50 conversations listed | < 150 MB |
| Idle CPU | ~0 % |

## Files (P9, P5)

- `~/.config/leon/config.toml`, `<project>/.leon/config.toml` — TOML, versioned.
  A bad value warns and uses the default; the app always starts.
- App data: SQLite (`rusqlite`, WAL) in the platform data folder.
