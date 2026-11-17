# How Leon works

A plain-language tour, for reviewing what was built and the choices behind
it. The detailed, always-current reference is
[spec/architecture.md](../spec/architecture.md); the rules are in
[spec/decisions.md](../spec/decisions.md). State on 2026-11-17, branch
`v1-build`.

## In one paragraph

Leon is one native window (Rust + GPUI) that drives the coding CLIs you
already use (Claude Code, Codex, Copilot CLI) with your own logins. It never
calls a model API. Each conversation is one or more **agent sessions**, each
one a real CLI conversation that Leon starts, feeds and listens to. Leon
stores the transcripts, shows the git changes of the folder the agents work
in, follows the branch's pull request through `gh`, and lets agents start and
message other agents through a small tool server of its own.

## The pieces

```
 ┌──────────── leon (one process) ─────────────┐
 │  UI thread (GPUI)  ⇄  core (tokio)           │       claude -p …        (Claude Code)
 │   renders            owns all state,        ─┼──▶    codex app-server   (Codex)
 │   snapshots,         talks to the outside    │       copilot --acp      (Copilot CLI)
 │   sends intents      through "ports"         │       git …, gh …
 │                      tools.sock  ◀───────────┼───   leon mcp  (one per CLI session:
 └──────────────────────────────────────────────┘        Leon's tools for agents)
```

| Crate | Job |
|---|---|
| `leon-domain` | Plain types and rules: conversation, transcript entry, diff, PR, doctor report. No I/O. |
| `leon-core` | The only writer of state. Applies your intents and the CLIs' events, saves, publishes a snapshot. Defines the ports (traits) every outside system sits behind. |
| `leon-backends` | Drives the CLIs: Claude Code (stream-json), Codex (app-server JSON-RPC), Copilot CLI (ACP). |
| `leon-git`, `leon-github` | Run `git` and `gh`. |
| `leon-store` | SQLite file: projects, conversations, sessions, transcripts. |
| `leon-catalog` | Reads agent files, skills, commands, `.mcp.json` from your folders. |
| `leon-app-tools` | The tool server agents call (`start_agent`, `message_agent`…). |
| `leon-config` | Reads `~/.config/leon/config.toml`. |
| `leon-ui` | The window. Renders snapshots, sends intents; no logic. |
| `leon` | `main`: wires everything, then opens the window. |

## A message, from your keyboard to the CLI and back

1. You press Enter: `Workspace::send_composer` sends `Intent::Send`
   ([composer.rs:351](../crates/leon-ui/src/composer.rs)).
2. The core applies it: `Core::apply` → `send` → `deliver`
   ([app.rs:274, 450, 463](../crates/leon-core/src/app.rs)) records your
   message in the transcript and saves it.
3. If the session's CLI is not running, `ensure_live`
   ([app.rs:497](../crates/leon-core/src/app.rs)) asks the backends port to
   start it, resuming the CLI's own session id when there is one.
4. `Clis::start` ([lib.rs:48](../crates/leon-backends/src/lib.rs)) picks the
   CLI: `claude -p --input-format stream-json …`
   ([claude.rs:19](../crates/leon-backends/src/claude.rs)), or `codex
   app-server` / `copilot --acp --stdio` run by one shared JSON-RPC loop
   ([rpc.rs:35](../crates/leon-backends/src/rpc.rs)).
5. The CLI streams lines; each adapter turns them into the same events
   (text, tool call, question, turn end…), e.g. `Parser::parse`
   ([claude.rs:167](../crates/leon-backends/src/claude.rs)).
6. The core applies each event (`events::apply`,
   [events.rs:9](../crates/leon-core/src/events.rs)), saves what must
   survive a restart, and publishes a new snapshot
   ([app.rs:203](../crates/leon-core/src/app.rs)).
7. The window re-renders from it (`Workspace::on_snapshot`,
   [workspace.rs:180](../crates/leon-ui/src/workspace.rs)).
8. At the turn's end, `finish_turn`
   ([app.rs:675](../crates/leon-core/src/app.rs)) refreshes git, sends the
   answer back to the agent that asked (if any), else marks the
   conversation unread and sends a desktop notification.

Example: you click + next to a project, pick the planner on Claude Code and
type "Fix the retry bug" → the conversation opens on its Flow tab (You →
planner) → Leon starts `claude -p … --model claude-opus-5-5 --effort high
--agent planner` in the project folder → click the planner's box or tab to
read the answer as it streams → the turn ends → the Changes tab re-reads
`git` → the left pane shows the conversation as done. Right-click it there to
rename, pin, archive or delete it.

## Agents starting other agents

Every CLI session gets Leon's tool server: the CLI starts `leon mcp` (Leon's
own program, as a tiny bridge) which forwards to `tools.sock`.

1. The planner calls `start_agent(agent: "coder", prompt: "Implement step
   1")`. The call reaches `Core::on_tool`
   ([agents.rs:16](../crates/leon-core/src/agents.rs)).
2. `start_agent` ([agents.rs:65](../crates/leon-core/src/agents.rs)) creates a
   coder session in the same conversation (same CLI, model and permission as
   the planner), sends it the prompt as `[From planner · session 1]`, and
   answers the tool at once: "Started coder as session 2".
3. When the coder's turn ends, `after_turn`
   ([agents.rs:209](../crates/leon-core/src/agents.rs)) sends its answer to
   the planner as a new message `[From coder · session 2]`; if the planner is
   busy, it waits until the planner's turn ends.
4. If you type in the coder's tab, the coder answers you, not the planner.
5. The Flow tab draws boxes and arrows from the sessions' `started_by` and
   their transcripts; nothing is hardcoded.

Agents can also call `rename_conversation` and `set_working_directory` (when
they move into a worktree, the Changes tab follows).

## Git and GitHub

- **git** (`git` command, read-only): uncommitted changes, branch diff from
  the merge base, branch name. Re-read when you open a conversation, click
  Changes or Branch, an agent edits a file or runs a command, and at turn end.
- **GitHub** (`gh` command, your login): the branch's PR (checks, reviewers,
  comments), your issues and PRs, "Request Copilot review", "Post comment".
  PR badges refresh every 2 minutes.
- **My issues** are grouped by their GitHub Project column (Todo, In
  progress…), in the board's order. A click opens the issue on GitHub;
  "Start conversation" opens the new-conversation form in the project whose
  `origin` remote is the issue's repository, with the issue's title and link
  as the first message.
- Clicking a file in Changes or Branch opens a drawer with every file's
  diff, at the one clicked. Press a line's + to comment, or drag it over
  several lines; comments pile up into one review sent from the drawer.
- Settings (bottom left) edits `config.toml`: defaults for new
  conversations, diff folding, source folders, flow arrows.
- Connectors (bottom left) shows, per CLI, its plugins, its own MCP servers
  and its account's connectors (Slack, Gmail…), read from the CLIs.

## Where things live

| What | Where |
|---|---|
| Settings | `~/.config/leon/config.toml`: `[defaults]`, `[diff] fold`, `[sources] folders` |
| Conversations | `~/Library/Application Support/leon/state.db` (macOS), `~/.local/share/leon/` (Linux) |
| Tool socket | `tools.sock` next to `state.db` |
| Agents, skills, commands | `~/.agents/`, `~/.claude/`, your `[sources]` folders, the project's `.agents/`, `.claude/`, `.github/` |

## The Doctor

Bottom of the left pane. Checks, in the background at start and on demand:
Leon's version and OS, the config file and its problems, the data folder,
your login-shell `PATH`, notifications, the tool socket, your source
folders; then each CLI (installed, version against the one Leon was tested
with, logged in), `git`, and `gh` (installed, logged in). Each problem says
what to do, e.g. "Run `claude auth login` in a terminal".

## Choices to review

What Leon does where the first decisions did not say, or said otherwise. All
accepted by you on 2026-11-17 and recorded in
[spec/decisions.md](../spec/decisions.md) (the "Decision" column).
"Reliability" notes what could break.

| # | Choice | Alternative | Decision | Why / reliability |
|---|---|---|---|---|
| 1 | GitHub through the `gh` CLI | GitHub's API over HTTPS | P7, D6 | More reliable; no HTTP library. |
| 2 | Agents reach Leon's tools through `leon mcp` (stdio bridge) + a local socket | An HTTP MCP server inside Leon | I1 | Every CLI supports stdio MCP servers; no new library. A second Leon refuses to start while one runs. Checked live with Claude Code and Codex. |
| 3 | Codex through `codex app-server` | `codex exec --json`, one process per turn | P2, I13 | Reliability: app-server is marked **experimental** by Codex. |
| 4 | Stopping Claude Code ends its process; the next message resumes it by session id | Claude's own interrupt message | I2 | Checked live: the resumed session still knows the conversation. |
| 5 | Copilot CLI over ACP (`copilot --acp --stdio`, preview); "auto" = `--allow-all-tools` | Its non-interactive mode | P2, I3 | Reliability: preview; never run here (not installed). |
| 6 | "Edit from here" / go back: drop later messages, start a new CLI session that gets the conversation so far as text | Each CLI's own rewind (Codex has one) | I4 | Same behaviour on every CLI. The new session loses tool details, not the conversation. |
| 7 | Titles: one short extra CLI call per new conversation (Claude: haiku; Codex: `exec`, low effort) | Keep the first line of the first message | I5 | Costs a little quota. Checked live with both. Copilot keeps the first line. |
| 8 | Leon's own tools run without a permission prompt | Ask each time | I6 | Otherwise every `start_agent` would stop for a click. Your own MCP servers keep their usual prompts. |
| 9 | A started agent uses the caller's CLI, model, effort and permission | Settings from the agent file | I7 | |
| 10 | An answer to another agent raises no notification | Notify for every turn | I8 | You are notified when something is for you. |
| 11 | CLIs idle for 10 minutes are stopped, resumed on the next message | Keep them all alive | I9 | Many conversations would otherwise keep many processes. |
| 12 | Source folders: Claude gets `--plugin-dir`; Codex and Copilot get only their `.mcp.json` servers | Copy files into each CLI's folders | I10 | Skills from source folders do not reach Codex or Copilot. Checked live: Claude loads a folder's skills, manifest or not. |
| 13 | The `PATH` is read once from your login shell | The inherited `PATH` | I11 | Started from the Finder, an app does not see `/opt/homebrew/bin`. |
| 14 | "My issues" / "My open PRs" at the top of the left pane | Elsewhere | GH-2 | |
| 15 | Pending review comments live in the window only (lost on restart) | Saved in the database | I12 | |
| 16 | The Doctor | — | DOCTOR-1 | |
| 17 | Settings write `config.toml` in place with `toml_edit` (comments kept, default values removed) | Rewrite the whole file | ⚠️ to confirm | You did not answer; the recommended option was taken. `toml_edit` was already in the build. A file that does not parse is never touched. |
| 18 | Icons: Lucide SVGs copied into `assets/icons` and embedded | The `gpui-kit-assets` crate | ⚠️ to confirm | No new dependency; ISC licence file kept next to them. |
| 19 | A conversation started from an issue stores it (`conversations.issue`, store migration 5) | Find the link from branch names or PR bodies | ⚠️ to confirm | Exact and simple; only conversations started from the issue list are linked. |
| 20 | Issue priority also read from an organization issue field named "Priority" | Board fields only | ⚠️ to confirm | On a real account, priority lived there and not on the board. |
| 21 | Connectors are read-only (each CLI's list commands, JSON) | Enable, disable, install from Leon | ⚠️ to confirm | Managing plugins changes your CLI setup; Codex has no command to enable or disable one. |
| 22 | `cargo run` starts Leon (`default-members`) | `cargo run -p leon` | ⚠️ to confirm | Plain `cargo build` / `cargo test` now cover the app crate only; CI passes `--workspace`. |

## Not built, and why

- **Scheduled tasks (SCHED-1)**: what runs, when, and whether Leon must be
  open are still open questions in features.md.
- **Plan limits in usage (USE-1)**: which numbers to show is open; tokens and
  cost per session are shown in Info.
- **The conversation's issue in Info (CTX-1)**: nothing tells Leon which
  issue a conversation is about.
- **Bind / unbind a PR (P10)**: the PR is found from the conversation's
  branch; agents have no tool to set it by hand yet.
- **Git buttons (GIT-6)**: listed under Later.

## Not verified yet

- Copilot CLI: not installed here.
- Claude Code's suggested replies (CONV-10): the message format was read in
  Claude Code's own code, not seen in a live run yet.
- Linux, and the GitHub Actions workflow (never run).
