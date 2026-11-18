# Decisions

Owned by the maintainer. Agents follow them and never add or change one: they
ask. When a decision changes, it is rewritten with the new date; history stays
in git.

## Decided

**D1 — Models only through the CLIs, on subscriptions.** (2026-11-17)
The app never calls a model API and holds no API key. Small AI tasks (e.g. a
conversation title) also go through a CLI call.
Why: the CLIs run on the user's subscriptions, with no per-call API cost; they
own login, the agent loop and tools.

**D2 — Who owns what.** (2026-11-17)
The maintainer owns `AGENTS.md`, `features.md` and `decisions.md`. Agents own
the code and `architecture.md`, and ask the maintainer for anything the first
three don't settle.

**D3 — Worktrees are created by agents.** (2026-11-17, changed 2026-11-18)
The app doesn't create worktrees. When an agent moves into a worktree, the app
follows (how: P10) and records the base branch and commit. The app never
deletes a worktree.
Changed 2026-11-18, in the maintainer's words: "one flow can edit several
repo, have several PRs, be linked to several issue, have several worktree.
All this should be doable and manageable and follow what the agent are
doing". A conversation is no longer limited to one working directory.

**D4 — Git changes go through agents.** (2026-11-17)
Commit, push, rebase… are asked to the agent in the conversation. The app only
reads git. Later buttons (GIT-6) just send that request as a message.

**D5 — Agents start other agents in new sessions, not as CLI subagents.**
(2026-11-17)
When a planner starts a coder, the coder gets its own CLI conversation, which
the user can open and talk to. All sessions that come from the user's first
message form one conversation in the left pane. CLI subagents still work but
are shown only as counts (CONV-13). How the call works: P15.

**D6 — Prefer the reliable solution.** (2026-11-17)
When two designs work, take the more reliable one: an official CLI over a
hand-written API client, a proven mechanism over a clever one.
Why: the maintainer would rather have something that keeps working than
something slightly faster or more direct (e.g. P7: `gh` over GitHub's API).

**D7 — Supported CLIs: Claude Code, Codex, Copilot CLI.** (2026-11-17)
No Google CLI (Antigravity is not supported; see P11).

**D8 — A left-pane entry is a Flow.** (2026-11-18)
In the maintainer's words: "Let's call an entry in the left pannel a 'Flow'.
Each flow can contain several agent conversation (as it is already the case)
But I also want to be able to start a new conv in an existing flow, which will
lead to a new input box and etc. This new naming/concept can appear in the
leon's UI if needed". Entries above and below written before this use
"conversation" for a flow and "agent session" for a conversation.

## Accepted on 2026-11-17

Proposals P1–P15, accepted as written by the maintainer on 2026-11-17. Their
ids are kept so references stay valid. Facts checked on 2026-11-17 unless
marked otherwise.

**P1 — UI: GPUI + gpui-component.**
Pin one exact snapshot; GPUI used only in the UI crate; a short local "UI
recipes" doc for agents. The M0 spike must meet the speed targets (architecture.md) on
macOS and Linux, otherwise fall back to Tauri 2.
Why: fastest and lightest option; `gpui-component` (Apache-2.0, 0.10.0) has the
virtual list, markdown, highlighted diff, tree and resizable panes; Zeron and
Waku, apps of the same kind, run on it.
Risk: API churn (official `gpui` crate stuck at 0.2.2; snapshots published as
`gpui-pre` 0.3.13) and coding agents know it poorly.
Alternatives: Tauri 2 (best known by agents, heavier, weak Linux rendering
through WebKitGTK); Iced (no diff view); Slint (no code or diff view); egui
(hard to make look modern).

**P2 — How the app talks to each CLI.**
Claude Code: `stream-json` in and out. Codex: `codex app-server` (JSON-RPC).
Copilot CLI: ACP. Inside the app, one event model shaped like ACP's.
Why: ACP is stable (v1) and native in Copilot CLI; Claude and Codex reach it
only through Node adapters that lag their features; quotas and agent choice
need per-CLI code anyway. Codex marks app-server as experimental; accepted
(see I13).
Alternatives: ACP everywhere through adapters; each CLI's native protocol only.

**P3 — Agents, skills and MCP defined once.** (extended 2026-11-18: generic
files in `~/.agents/`, I15)
The user registers source folders in the plugin layout (`agents/`, `skills/`,
`.mcp.json`, optional `.claude-plugin/plugin.json`). Each CLI reads natively
what it can (skills in `~/.agents/skills`, plugin manifests). The app passes the
chosen agent and the MCP list when a conversation starts. It never writes into
the CLIs' own folders. MCP OAuth: each CLI logs in once per server.
Why: skills and plugin manifests already converge across CLIs; agent formats
and MCP config locations don't.
Alternatives: copy/convert files into each CLI's folders; an app-run MCP
gateway that logs in once for every CLI (more to build).

**P5 — Storage.**
SQLite holds the app's data and a copy of each conversation's events, in the
app's own format. The CLI keeps its transcript for resuming; the app never
parses it.
Why: four private transcript formats would break with every CLI update.

**P6 — git.** Run the `git` command with its machine-readable output.
Why: behaves exactly like the user's git (config, worktrees); no C or large
dependency. Alternatives: `git2` (C library), `gix` (still maturing).

**P7 — GitHub through the `gh` CLI.** (changed 2026-11-17)
Every GitHub read and action runs `gh` with the user's own login (`gh api
graphql` for reads); the app never reads the token. PR badges refresh with one
`gh` call per conversation every 2 minutes.
Why: more reliable (D6): `gh` owns login, hosts and rate limits; no HTTP
library in the app.
Alternative: call GitHub's API directly with the token from `gh auth token`
(one batched request for every badge, but the app owns auth and HTTP).

**P8 — Concurrency.** The core runs on tokio; GPUI stays in the UI crate; they
talk through channels. Why: the core stays independent of the UI framework.

**P9 — Config.** TOML at `~/.config/leon/config.toml` (macOS too), per-project
`.leon/config.toml`. App data in the platform data folder.

**P10 — Tools the app gives agents.** A small local MCP server added to every
agent session: start another agent and message it (P15), rename the
conversation, bind/unbind a PR, tell the app the session moved to a worktree or
another repo (D3).
Why: your workflow uses the Claude desktop app's equivalents
(`set_session_title`, `bind_pr`, `EnterWorktree`). Without it, the app can only
guess title, PR and folder from the CLI's events.

**P11 — No Google CLI.** (changed 2026-11-17)
Antigravity (`agy`) is not supported, nor Gemini CLI (D7).
Why: the maintainer does not want it.

**P12 — Build, don't fork Zeron.**
Zeron (MIT, GPUI, 3.7k ★) covers much of the same scope. Build our own, study
its notes, and reuse MIT/Apache code with attribution where it fits (e.g. the
`claude-codes` crate for Claude's `stream-json` types).

**P13 — Licence:** `AGPL-3.0-only`. (changed 2026-11-18)
In the maintainer's words: "I want an open one, but still avoid someone
stealing my idea which is free and public just make full money outside of
it", then "AGPL-3.0". Anyone who distributes a changed Leon, or offers it as
an online service, must publish their changes under the same licence.

**P14 — Name:** `leon` (binary and config folder).

**P15 — How one agent starts another (FLOW-1, FLOW-2).**
The app's local MCP server (P10) gives every agent session two tools:
`start_agent(agent, prompt, working_directory?)` and
`message_agent(session, text)`. Both return at once. When the other session
finishes its answer, the app sends that answer as a new message to the session
that asked. The app records each call (who, to whom, prompt, answer) and draws
the flow from them.
Why: an implementation can run 30+ minutes; waiting inside one tool call would
hit tool-call timeouts (⚠️ not verified per CLI) and keep the caller blocked.
Returning at once works the same on every CLI (it only needs MCP and sending a
message), and the user can talk to the coder in the meantime.
Alternative: the tool waits for the answer, like a subagent call (simpler for
the calling agent, but timeouts and a blocked caller).

## Implementation choices, accepted on 2026-11-17

Made while building v1 where the decisions above were silent, reviewed and
accepted by the maintainer on 2026-11-17 (list in
[docs/how-leon-works.md](../docs/how-leon-works.md#choices-to-review)).

**I1 — How agents reach the app's tools (P10).** Each CLI starts `leon mcp`,
the app's own program as a stdio MCP server, which only relays to a Unix
socket the app listens on (`tools.sock` in the data folder). A second app
finding the socket in use refuses to start.
Why: every CLI supports stdio MCP servers; no HTTP server in the app.

**I2 — Stopping Claude Code.** Stop ends its process; the next message
resumes the CLI session by its id.

**I3 — Copilot CLI.** Started as `copilot --acp --stdio`; the "auto"
permission is `--allow-all-tools`; an agent's instructions go in ACP's system
prompt when Copilot offers it, else before the first message.

**I4 — Going back / editing a message (CONV-7, CONV-8).** Later entries are
dropped and a new CLI session starts with the conversation so far as text.
The same on every CLI, instead of each CLI's own rewind.

**I5 — Titles (NAV-9, D1).** One short CLI call per new conversation:
Claude Code `-p` with `haiku`, Codex `exec` with low effort. Copilot CLI keeps
the first line of the first message.

**I6 — The app's tools run without a prompt.** Claude Code
`--allowedTools mcp__leon`, Codex per-server approval, Copilot CLI
`--allow-tool`. The user's own MCP servers keep their usual prompts.

**I7 — A started agent's settings.** (changed 2026-11-18) An agent started by
another runs on the CLI and model its file's `model` picks (I17); without
one, or with a model no CLI offers, on the caller's CLI, model, effort and
permission. On another CLI than the caller's, the effort stays when that CLI
has it and the permission becomes that CLI's "auto".

**I8 — No notification for answers between agents.** NOTIF-1 fires when an
answer is for the user.

**I9 — Idle CLIs.** A CLI process idle for 10 minutes is stopped and resumed
by its session id on the next message.

**I10 — Source folders per CLI (P3).** (changed 2026-11-18) Claude Code loads
each folder with `--plugin-dir`; Codex and Copilot CLI get the folders'
`.mcp.json` stdio servers, and their skills through the skills bridge (I15).

**I11 — `PATH`.** Read once from the user's login shell and given to every
program the app starts, so an app started from the Finder finds the CLIs.

**I12 — Pending review.** Diff comments waiting to be sent live in the
window only; they are lost when the app quits.

**I13 — Codex app-server.** Kept although Codex marks it experimental (P2).

**I14 — Leon's shared MCP list (TOOLS-1, EXT-2).** Servers added in the Tools
view go to `mcp.json` (the `.mcp.json` shape) in Leon's config folder, owned by
Leon, and reach every CLI's sessions: Claude Code through `--mcp-config`,
Codex through its thread config, Copilot CLI through ACP. Local (stdio)
servers only. Each CLI's own servers are shown read-only and changed in the
CLI. Chosen by the maintainer on 2026-11-17 (P3 unchanged).

**I15 — Generic agent and skill files (EXT-4, EXT-5).** Leon's home for them
is `~/.agents/` (`agents/<name>.md`, `skills/<name>.md`) and a project's
`.agents/`; a skill may also be a `<name>/SKILL.md` folder. Leon also reads
`.claude/agents/` and `.github/agents/`; on a name clash `.agents/` wins.
Leon bridges the files to each CLI: the skills a CLI does not read itself are
copied at each session start into Leon's data folder (`skills/<cli>/…`, never
the CLIs' own folders, P3) and handed over with `--add-dir` (Claude Code),
`skills/extraRoots/set` (Codex), `--plugin-dir` (Copilot CLI). Every CLI
still loads a skill only when the agent needs it; there is no skills field,
an agent names a skill in its text. Chosen by the maintainer on 2026-11-18.

**I16 — An agent's `tools` (EXT-7).** In GitHub's custom-agent words (`read`,
`search`, `edit`, `execute`, `web`, `agent`, `todo`; Claude Code's tool names
understood too). They only take built-in abilities away, mapped per CLI as
`docs/agent-files.md` lists; without `tools` the agent has every tool its CLI
has, and MCP tools are never limited. Chosen by the maintainer on 2026-11-18.

**I17 — An agent's `model` picks the CLI (EXT-6).** The first CLI that offers
it, in an order the user sets in Settings (`[agents] cli_order`, default
Claude Code, Codex, Copilot CLI), for the new-conversation form and for an
agent another agent starts (I7). Chosen by the maintainer on 2026-11-18.

**I18 — Shared instructions (EXT-8).** `~/.agents/AGENTS.md`, read at each
session start, goes to every agent session: Claude Code through
`--append-system-prompt`, Codex before the agent's instructions in
`developerInstructions`, Copilot CLI through `COPILOT_CUSTOM_INSTRUCTIONS_DIRS`
(it then reads the file itself). Project `AGENTS.md` files are left to the
CLIs. Each CLI's own personal file (`~/.claude/CLAUDE.md`,
`~/.codex/AGENTS.md`, `~/.copilot/copilot-instructions.md`) still loads in
that CLI. Chosen by the maintainer on 2026-11-18.

