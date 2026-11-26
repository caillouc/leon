# Architecture

How the code is organised. Owned by agents; must follow
[decisions.md](decisions.md). Follows D1–D5 and P1–P15
(accepted 2026-11-17).

## Processes

- One app process: UI thread (GPUI, P1), core on tokio (P8), and the app
  tools' MCP server (P10) listening on a Unix socket in the data folder
  (`tools.sock`, folder mode 0700). A second Leon finding it in use stops.
- Each CLI starts `leon mcp` (the app's own binary) as a stdio MCP server; that
  bridge only pipes bytes between its stdio and the socket, after one line
  naming its conversation (`LEON_SESSION`, `LEON_SOCKET` in its environment).
- One CLI process per conversation, under its **holder** (I24): `leon hold
  <data>/runs/<conversation> -- <cli> <args…>`, the app's own binary, in its
  own process group so it outlives the app (`leon-backends`: `holder.rs`
  runs in the holder, `conn.rs` is the app's side; a deleted conversation's
  holder is stopped and its folder removed, `conn::discard`, and its Claude
  Code session files go, `sessions.rs`, and its images, `images.rs`). The
  images of a message (CONV-18, I27) are saved by `Backends::save_image` in
  `<data>/images/<conversation>/` when it is sent, then each adapter gives
  them to its CLI (base64 for Claude Code and Copilot CLI, the path for
  Codex); the message box holds them meanwhile (`leon-ui/src/attach.rs`).
  The holder keeps the
  CLI's stdin open, appends its stdout to `out` and stderr to `err`, takes
  the app's lines on the Unix socket `sock` (`>line` to stdin, `!stop`),
  and writes `exit` once the CLI has ended and its output is whole. The app
  reads `out` as it grows (every 30 ms when quiet) and writes through the
  socket. It also keeps two marks there: `busy` while a turn runs (set on a
  message, cleared on its end) and `turn`, the offset in `out` where that
  turn began. When the app's last connection goes and no turn runs, the
  holder ends the CLI and itself, so only working agents outlive the app.
  At start, the core asks for the conversations with a folder in `runs`,
  reconnects to each live holder, and for a busy one truncates the transcript
  to the message that started the turn and replays `out` from `turn`, marked
  working. A socket path must stay under the Unix limit (104 bytes on
  macOS): `runs` sits in the data folder for that. Without holders (tests),
  the CLI is the app's own child, on pipes. A CLI crash sets that
  conversation to `error`; it never takes the app down.
- `git` and `gh` spawned per query (P6, P7). GitHub goes through the `gh` CLI,
  not direct HTTPS calls (P7, D6).
- Every adapter that starts a program gets the user's login-shell `PATH` from
  the binary (`leon_core::ports::SearchPath`), read once on first use: an app
  started from the Finder inherits a `PATH` without `/opt/homebrew/bin`.

## Crates and layers

Ports and adapters: dependencies point inward, to the core and the domain.
`cargo xtask layers` checks it in CI (table in `xtask/src/layers.rs`).

| Ring | Crate | Does | May depend on |
|---|---|---|---|
| 1 | `leon-domain` | Pure types and rules: ids, projects, settings; later conversation state, events, status/unread rules, PR badge, fold matching | nothing internal |
| 2 | `leon-core` | Owns state, publishes snapshots, defines the ports (traits); later starts/resumes conversations and routes events | domain |
| 3 | adapters: `leon-config` (TOML → settings; writes the user's file for Settings: `ConfigFile`), `leon-store` (SQLite: `Storage`), `leon-backends` (Claude Code stream-json; Codex app-server and ACP for Copilot CLI through one JSON-RPC runner: `Backends`), `leon-catalog` (agent files: `Catalog`), `leon-git` (`git` command: `Git`), `leon-github` (`gh` command: `GitHub`), `leon-app-tools` (MCP server for agents: `AppTools`, tool calls sent to the core as `AppToolRequest`s) | Implement one port each against the outside world | domain, core; never another adapter |
| 3 | `leon-ui` | Views, input → intents. The only crate that uses GPUI | domain, core |
| 4 | `leon` (bin) | Builds the adapters, starts the core, starts the UI; Leon's own facts for Doctor (`about.rs`) and updating itself (`update.rs`: `Updater`) | any |
| — | `xtask` | `cargo xtask ci`, `cargo xtask layers` | no Leon crate |

A new crate goes into both this table and `xtask/src/layers.rs`, or CI fails.

## Data flow

```
UI intent ─▶ core command ─▶ Backend port ─▶ CLI
CLI output ─▶ adapter ─▶ domain Event ─▶ apply(state, event) ─▶ store ─▶ UI notified
```

- The core is the only writer of state; the UI reads snapshots.
- `apply(ConversationState, Event) -> ConversationState` is pure (in `leon-domain`);
  status, unread and usage derive from events. Replaying stored events rebuilds
  a conversation.
- A flow's status and unread are derived from its conversations
  (running if any runs, `needs_input` if any waits, and so on).

## Conversations (D5, P15)

```
Flow (folders: every working tree its agents work in, D3, CTX-2)
 ├─ Conversation  planner   (started_by: none, the user)
 ├─ Conversation  coder     (started_by: planner)
 ├─ Conversation  reviewer  (started_by: planner)
 └─ Conversation  tester    (started_by: none, the user, with "+ New": FLOW-7)
```

In code: `Flow` (`FlowId`, SQLite table `flows`) and `Conversation`
(`ConversationId`, table `conversations`, its transcript in `entries`).

1. Every conversation's CLI gets Leon's MCP server: Claude Code through
   `--mcp-config` and `--allowedTools mcp__leon`, Codex through `thread/start`
   `config.mcp_servers.leon` with `default_tools_approval_mode = "approve"`
   (checked live with Codex 0.160.0 and Claude Code 2.1.288 on 2026-11-17:
   both call Leon's tools without a prompt).
2. The planner calls `start_agent(agent?, prompt, working_directory?)`; without
   `agent`, the CLI's own agent, without an agent file. The core
   (`crates/leon-core/src/agents.rs`) creates the coder's conversation in the
   same flow with the planner's CLI, model, effort and permission, sends the
   prompt as `[From planner · conversation N]` and answers the tool at once.
   Names are the ones the user sees (`State::conversation_name`: the agent,
   numbered when the flow has several: `coder 2`), in the UI and for the
   agents alike; `message_agent` takes a name or an id.
3. The coder's transcript records the prompt as `Entry::FromAgent { answer:
   false }`; the coder's `reply_to` is the planner.
4. When the coder's turn ends, its answer (its agent messages since the
   prompt) is sent to the planner as `Entry::FromAgent { answer: true }`, now
   if the planner is idle, else when its turn ends (the conversation's inbox).
   Answers between agents raise no notification. The inbox and the user's
   queued messages (CONV-19, sent as one) are saved at each change in SQLite
   `waiting_messages` (I30, `crates/leon-core/src/waiting.rs`); at startup
   each conversation gets them back, and one idle by then gets its first at
   once. A conversation found still in its turn gets its `reply_to` back from
   its transcript (`agents::works_for`).
5. `message_agent(conversation, text)` follows the same path. A message the
   user types in the coder's conversation is answered to the user.
6. `rename_flow` edits the flow. A folder joins the flow
   (`crates/leon-core/src/folders.rs`) when an agent calls
   `set_working_directory`, or when its CLI reports a file edit
   (`BackendEvent::FilesChanged`): the core asks git for the working tree
   holding the file (`Git::working_tree`, a repository or one of its
   worktrees) and adds it once; a conversation started with a
   `working_directory` brings it at once, unless that is a repository's own
   checkout (`folders::Came`: a main checkout joins when an agent changes a
   file there or says it works there). Each conversation keeps the folder
   it last worked in (`ConversationView::folder`), which the right pane shows
   on its tab, saved in `conversations.folder` (I20). The right pane shows
   only the folders a conversation works in; the folder a flow started in,
   before any joined, only when it is not a repository's own checkout
   (`GitView::main`). The folder the calling CLI
   was started in never changes (its resume depends on it).
7. Git state (`GitView`) and pull requests are kept per folder. A flow's
   links (`Flow::links`, SQLite `flow_links`, I19) are the issue it was
   started for and what its agents linked with `link_github`
   (`crates/leon-core/src/links.rs`), each read from GitHub when linked, at
   startup and at each tick. Its pull requests are its folders', then its
   linked ones; its issues its linked ones, then those its pull requests
   close (`closingIssuesReferences`). The right pane shows one folder at a
   time (`crates/leon-ui/src/right_folders.rs`).
   A flow's to-dos (TODO-1, `leon_domain::Todo`, SQLite `todos`) are added by
   the user (`Intent::AddTodo`) or an agent (`add_todo`, the author kept as
   its conversation), marked done by the user, all loaded at startup
   (`crates/leon-core/src/todos.rs`); the UI shows them in Info, behind the
   main pane's top-right button (the To do page, `Screen::Todos`) and as a
   count on flow rows (`crates/leon-ui/src/todos_view.rs`).
   Whom a conversation's answers go to — the agent whose request it works
   on, or the user — is one rule over its transcript,
   `leon_domain::answers_to`: the core routes answers by it
   (`crates/leon-core/src/agents.rs`) and the Flow tab draws its arrows by
   it (`crates/leon-ui/src/flow_model.rs`).
8. Every conversation is told, in its CLI's system prompt or first message, to
   keep Leon up to date with these tools (`AppTools::instructions`, CTX-3;
   assembled in `crates/leon-backends/src/told.rs`).
9. The flow diagram (FLOW-4) is built from the conversations' `started_by`,
   their agents and their transcripts only (`crates/leon-ui/src/flow_model.rs`):
   boxes placed by `flow_model/force.rs` (CS Academy's force rules, from
   the stars `flow_model/start.rs` lays out around each box; sizes and
   geometry in `flow_model/layout.rs`), arrows drawn by
   `flow_model/arrows.rs` (straight, bundled by pair of boxes, labels in free
   spots).

## Events (sketch, shaped like ACP session updates — P2)

`SessionStarted` · `UserMessage` · `AgentTextDelta` / `AgentMessage` ·
`ThinkingDelta` · `ToolCall` / `ToolCallUpdate` · `PermissionRequest` ·
`Question` · `Usage` · `WorkingDirChanged` · `TurnEnded` · `BackendError`.
Persisted (P5), so versioned with migration tests.

## CLIs (P2)

`local` = checked on the installed CLI's `--help` on 2026-11-17; others from
official docs the same day, re-checked by contract tests before each adapter
ships.

| | Claude Code | Codex | Copilot CLI |
|---|---|---|---|
| Checked on | 2.1.288 local | 0.160.0 local | docs + ACP schema v1.32.0, 2026-11-17; 1.0.114 local: live tests 2026-11-18, a permission request recorded 2026-11-22 (`fixtures/copilot/1.0.114`) |
| Transport | `-p --input-format stream-json --output-format stream-json --include-partial-messages` (local) | `codex app-server` JSON-RPC, marked experimental; schema from `generate-json-schema` (local) | `copilot --acp --stdio` (preview) |
| Agent | `--agent`, `--agents <json>` (local) | No `--agent` flag (local) → agent's instructions in `thread/start.developerInstructions` (local schema) | instructions in ACP `systemPrompt` when advertised, else before the first prompt |
| Model / effort | `--model <full id>`, `--effort`; Leon always passes both. Models asked at launch (I31, `leon-backends/src/models.rs`): the `initialize` control request's `models`, by `resolvedModel`, the `default` one first (2.1.332); Claude Code reports no default effort, Leon uses `high` | `turn/start.model`, `turn/start.effort`; `model/list` default `gpt-6-luna`, effort `medium` | `--model=`, `--effort=` flags ⚠️ with `--acp`; Leon uses `medium` |
| Stop (CONV-6) | end the process; the next message resumes by session id (I2; run 2026-11-17: context kept) | `turn/interrupt` (local schema) | ACP `session/cancel` |
| Go back (CONV-7) | no flag in `--help` (local) ⚠️ — fallback: new session carrying the messages up to that point | `thread/revert { beforeTurnId }` (local schema) | ⚠️ |
| Subagent counts (CONV-13) | `parent_tool_use_id`; `--forward-subagent-text` (local) | `CollabAgent*` items (local schema) | ⚠️ |
| Suggested replies (CONV-10) | `--prompt-suggestions` → `prompt_suggestion` message (local) | ⚠️ | ⚠️ |
| Questions and approvals (CONV-9) | `--permission-prompt-tool stdio`: `control_request` `can_use_tool` on stdout (`AskUserQuestion` or the tool to approve), `control_response` `allow` (with `updatedInput.answers` for questions) or `deny` on stdin (run 2026-11-20); without the flag `AskUserQuestion` is not offered and approvals are refused (I23) | `item/commandExecution/requestApproval`, `item/fileChange/requestApproval` (local schema) | ACP `session/request_permission` |
| Usage limits (DOCTOR-2) | `rate_limit_event` on stream-json: `rate_limit_info.unifiedWindows.five_hour` / `.seven_day` (`utilization` 0–1, `resetsAt` s), else the top-level window (run 2026-11-20) | `account/rateLimits/updated`: `rateLimits.primary` / `.secondary` (`usedPercent`, `windowDurationMins`, `resetsAt` s), `.credits` (recorded 0.160.0); without a turn, `account/rateLimits/read` from a short `codex app-server` (`plan.rs`, run 2026-11-24) | none over ACP; its session events (`session.usage_checkpoint`: AI credits, premium requests) are per session, its plan budget and limits only in `/usage` (1.0.114, 2026-11-24) |
| Title (NAV-9) | one-off `claude -p --model haiku --output-format text` (run 2026-11-17) | one-off `codex exec --skip-git-repo-check --ephemeral -s read-only` (run 2026-11-17) | none: the first line stays |
| Permission | `--permission-mode acceptEdits\|auto\|bypassPermissions\|manual\|dontAsk\|plan` (local) | `-a`, `-s`, `--dangerously-bypass-approvals-and-sandbox` (local) | `--allow-all-tools`, `--allow-all` (I3) |
| MCP list | `--mcp-config` + `--allowedTools mcp__leon` (local) | `thread/start.config.mcp_servers`, Leon's with `default_tools_approval_mode = "approve"` (live, 0.160.0) | ACP `mcpServers`; `--allow-tool=leon` ⚠️ |
| Resume | `--resume`, `--fork-session` (local) | `thread/resume`, `thread/fork` | ACP `session/resume`, else `session/load`, else a new session with a notice |
| Usage | `result.total_cost_usd`, `rate_limit_event` | `thread/tokenUsage/updated`, `account/rateLimits/read` | `/usage` |

Rules for every adapter:

- Always pass model, effort, permission and agent explicitly. Claude Code's docs
  announce `-p` defaulting to manual mode (v2.1.302) and `--bare` becoming the
  default ⚠️ not verified locally.
- Declare the supported CLI version range; check `--version` at startup.
- Pickers show what the CLI itself offers.

## Agents, skills, MCP (P3)

Where each CLI looks (docs, 2026-11-17):

| | Claude Code | Codex | Copilot CLI |
|---|---|---|---|
| Instructions | `CLAUDE.md` (`AGENTS.md` only if no CLAUDE.md) | `AGENTS.md` | `AGENTS.md`, `CLAUDE.md` |
| Skills | `~/.claude/skills`, plugins | `~/.agents/skills` | `~/.agents/skills`, `.github/skills`, `.claude/skills` |
| Agents | `.claude/agents/*.md` | TOML in `.codex/agents/` | `*.agent.md` |
| Plugins | `.claude-plugin/` | reads Claude manifests | reads `.claude-plugin/` |

`leon-catalog` reads, in this order (a later entry hides an earlier one of the
same name): the user's folders (`~/.claude`, `~/.github`, then Leon's generic
`~/.agents`), the source folders listed in `[sources] folders` of the config
(plugin layout: `agents/`, `skills/`, `commands/`, `.mcp.json`), then the
project's folders in the same order. It feeds the agent picker, the `/` list
(CONV-11) and Settings' folder list. Agent files give `model` and `tools`
(EXT-4, EXT-6, EXT-7): `leon_domain::agent_file` holds the tools vocabulary
and the model-to-CLI pick, `leon_core::settings_for_agent` applies it to a
conversation's settings (the form, and an agent another agent starts).

Each conversation gets the source folders, Leon's generic skills (EXT-5) and
`~/.agents/AGENTS.md` (EXT-8: `--append-system-prompt` for Claude Code, first in
Codex's `developerInstructions`, `COPILOT_CUSTOM_INSTRUCTIONS_DIRS` for Copilot
CLI).
`leon-backends/src/bridge.rs` copies the skills a CLI does not read itself
into `<data folder>/skills/<cli>/<hash>/`, in that CLI's layout, at each
conversation start:

- Claude Code: `--plugin-dir <folder>` per source folder (local `--help`,
  2.1.288), so its skills, commands, agents and MCP servers load natively
  (checked on 2026-11-17, with or without `.claude-plugin/plugin.json`);
  every `.agents/skills` skill through `--add-dir=<bridge>` (its
  `.claude/skills/` loads; checked live on 2026-11-18). An agent's missing
  tools become `--disallowedTools`.
- Codex: the folders' stdio MCP servers in `thread/start.config.mcp_servers`;
  `<name>.md` skills and source-folder skills through
  `skills/extraRoots/set`, sent before `thread/start`. Missing tools: a
  read-only sandbox, `features.shell_tool`, `web_search`,
  `features.multi_agent` in the thread config.
- Copilot CLI: over ACP it drops stdio MCP servers ("Rejecting non-http/sse
  MCP server", 1.0.113 log), so Leon's own tools and the folders' servers go
  in `--additional-mcp-config` when it starts (live-tested on 1.0.114); the
  same skills as Codex through `--plugin-dir=<bridge>`; missing tools as
  `--deny-tool=` rules (`read`, `write`, `shell`, `url`). It offers no ACP
  system prompt: an agent's instructions go before the first message.

Only stdio servers are passed; `.mcp.json` servers over HTTP are skipped.

## git (P6)

- Read-only commands with `--no-optional-locks`; the user's diff settings are
  overridden so the output always parses (`crates/leon-git`). The one write
  is `git fetch --no-write-fetch-head --no-auto-maintenance origin <base>`,
  with `GIT_TERMINAL_PROMPT=0`, run by the core at most every five minutes
  per folder (I22).
- Uncommitted: `git status --porcelain=v2 -z` for untracked files, one
  `git diff HEAD` for tracked ones; untracked files are read from disk.
- Branch diff: `git diff <merge-base(base, HEAD)> HEAD`; the base is
  `origin/<PR base>` when the branch has a PR and that ref exists, else
  `origin/HEAD`, else the first of `origin/main`, `origin/master`, `main`,
  `master` that exists.
- Read off the core's async workers, through `jobs::spawn`: blocking jobs
  (git, `gh`, the doctor) run a few at a time (6), so a burst of them at
  launch or on a tick never runs out of open files (a macOS app gets 256);
  refreshes of one
  flow coalesce. Triggers: opening the flow, clicking the
  Changes or Branch tab, an agent's edit or command finishing, the turn ending.
  No file watcher yet.
- The UI renders a diff as one virtualised list of rows (summary, file,
  hunk, line, comment); files matching `diff.fold` start folded. The drawer
  (`crates/leon-ui/src/diff_drawer.rs`) reuses the same list for one file,
  without the summary and file rows.

## GitHub (P7)

- `gh` with the user's own login; the app never reads the token
  (`crates/leon-github`). Reads are one `gh api graphql` query each.
- A flow's PR: read when the flow is opened (its linked PRs too), when its
  branch changes, after a review request or comment, every 30 s for the
  flow looked at while one of its checks runs, and every 120 s for every
  open flow (one `gh` call each, not batched yet). A PR read anywhere (its
  details, a folder's branch, a link, my list) replaces every copy the core
  keeps of it (`crates/leon-core/src/fresh.rs`), so no view lags another.
- My issues and PRs: read at startup and when the list is opened. Issues
  come with their GitHub Project "Status", "Priority" (a board single-select
  or an organization issue field) and iterations
  (`projectItems(includeArchived: false).fieldValues`); without the
  `read:project` scope the query is retried without them and issues show
  under "No status". A project's GitHub repository is read from its `origin`
  remote (`Git::github_repo`) to pick the project for an issue.
- A pull request's checks count each job once, its latest run: the most
  recently started per name, workflow and event, a status once per context,
  as `gh pr checks` does (`checks.rs`); GitHub's rollup keeps every run.
- Details (the drawer): one query for the issue (body, assignees, comments)
  or the PR (checks, reviewers, comments, description, unresolved review
  threads), by repository and number, read in
  the background (`Intent::ShowDetail`). A flow started from an issue
  keeps it (`flows.issue`, store migration 5).
- Ready / draft: `gh pr ready <n> [--undo] --repo <owner/name>` (flags from
  `gh pr ready --help`, gh 2.101.0, 2026-11-18; never run for real).
- Copilot review: `gh pr edit <n> --add-reviewer @copilot` (documented in
  `gh pr edit --help`, gh 2.101.0, 2026-11-17; never run for real).

## Settings and connectors

- Settings write `config.toml` in place with `toml_edit` (`leon-config`,
  `write.rs`): only the keys Settings own change; comments, order and
  unknown keys stay; a key equal to its default is removed. A file that does
  not parse is never rewritten. The new file replaces the old in one rename.
- Connectors (`Backends::connectors`, `crates/leon-backends/src/connectors.rs`):
  per CLI, `plugin list --json` and `mcp list --json` (Claude's `mcp list` is
  text; its `claude.ai …` lines are the account's connectors; Codex's runs
  with `--disable plugins`, plugins' servers being shown through their
  plugin). What a CLI ships or turns on itself is dropped by the readers
  (`connectors/parse.rs`), Claude's account connectors on every account
  (AccuWeather, Booking.com, Uber) by their hosts, `claude mcp list` not
  marking them. Read in the background each time the view opens:
  one task per CLI, its two commands at once; each answer lands in the
  snapshot on its own (`Snapshot::tools`), `Snapshot::tools_pending` naming
  the CLIs still asked.

## UI shell

- Icons: Lucide SVGs embedded in the binary (`assets/icons`, ISC licence),
  served to GPUI by `leon_ui::Assets` (`crates/leon-ui/src/icons.rs`).
- Menus and shortcuts: `crates/leon-ui/src/shortcuts.rs`; the workspace holds
  a focus handle so shortcuts work with no text box focused.
- Drawers (diffs, issue/PR details): `crates/leon-ui/src/drawer.rs`, over the
  window on a backdrop that blocks the mouse to everything behind.
- Every pane's top row is 48 px (`widgets::HEADER_HEIGHT`) so the dividers
  line up.

## Doctor

Every port that reaches an outside tool has a `doctor` method that checks it
(installed, version, logged in), each in its adapter; the binary adds Leon's
own facts (`crates/leon/src/about.rs`). The core runs them all off its
thread at start and on `Intent::RunDoctor` (`crates/leon-core/src/doctor.rs`)
and publishes the report in the snapshot; the coding CLIs are checked at the
same time. Versions are compared with the ones each adapter was checked with;
older ones are warnings.

Leon's own update (DOCTOR-3, I26): the core asks `GitHub::latest_release`
when Doctor runs and on the 2-minute tick once a day
(`crates/leon-core/src/update.rs`); the `Updater` port, in the binary
(`crates/leon/src/update.rs`), gives the repository (the workspace's
`repository`), the version running, whether it runs from an app bundle, and
on "Update" checks the `.dmg` that `GitHub::download_release` saved in the
data folder's `updates/` and swaps the bundle; "Restart Leon" starts a
detached wait that opens the bundle once this process ended, then the UI
quits (`Update::Restarting`).

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
| Idle memory, 50 flows listed | < 150 MB |
| Idle CPU | ~0 % |

## Files (P9, P5)

- `~/.config/leon/config.toml`, `<project>/.leon/config.toml` — TOML, versioned.
  A bad value warns and uses the default; the app always starts.
- App data: SQLite (`rusqlite`, WAL) in the platform data folder.
