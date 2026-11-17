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

**D3 — Worktrees are created by agents.** (2026-11-17)
The app doesn't create worktrees. A conversation has one working directory at a
time; when an agent moves into a worktree, the app follows (how: P10) and
records the base branch and commit. The app never deletes a worktree.

**D4 — Git changes go through agents.** (2026-11-17)
Commit, push, rebase… are asked to the agent in the conversation. The app only
reads git. Later buttons (GIT-6) just send that request as a message.

**D5 — Agents start other agents in new sessions, not as CLI subagents.**
(2026-11-17)
When a planner starts a coder, the coder gets its own CLI conversation, which
the user can open and talk to. All sessions that come from the user's first
message form one conversation in the left pane. CLI subagents still work but
are shown only as counts (CONV-13). How the call works: P15.

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
Copilot and Google: ACP. Inside the app, one event model shaped like ACP's.
Why: ACP is stable (v1, official Rust crate) and native in Copilot and
Antigravity; Claude and Codex reach it only through Node adapters that lag
their features; quotas and agent choice need per-CLI code anyway.
Alternatives: ACP everywhere through adapters; each CLI's native protocol only.

**P3 — Agents, skills and MCP defined once.**
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

**P7 — GitHub.** Call GitHub's API directly, with the token from
`gh auth token` or `GITHUB_TOKEN`; one batched request refreshes every PR
badge. Fall back to `gh` for an action the API lacks.
Alternative: run `gh` for every call (slow, no batching).

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

**P11 — Google CLI: Antigravity (`agy`) only.**
Why: Gemini CLI is being replaced by Antigravity CLI; since 2026-06-18 it no
longer serves free and AI Pro/Ultra users
([Google](https://developers.googleblog.com/an-important-update-transitioning-gemini-cli-to-antigravity-cli/)).

**P12 — Build, don't fork Zeron.**
Zeron (MIT, GPUI, 3.7k ★) covers much of the same scope. Build our own, study
its notes, and reuse MIT/Apache code with attribution where it fits (e.g. the
`claude-codes` crate for Claude's `stream-json` types).

**P13 — Licence:** `MIT OR Apache-2.0`.

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
