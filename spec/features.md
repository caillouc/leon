# Features

What the app does. Owned by the maintainer; agents read it and never edit it.
**Not listed here = not built.** Items marked *(proposal)* wait for the
maintainer's go.

## Vision

One native, fast window to run and follow many coding-agent conversations
across many repositories, on top of the coding CLIs the user already pays for by
subscription: Claude Code, Codex and Copilot CLI (D7).

- Talk to any agent directly. No generic agent in between.
- See how agents work together: which agent started which, what they told each
  other, and open any of their conversations.
- Bring your own agents, skills and tools in standard formats. Another user
  makes the app theirs by adding their own files.
- **The simplest solution that does the job.** No complex feature when a simple
  one works.
- UX reference: the Claude Code desktop app; Copilot in VS Code for picking an
  agent directly; Zed for speed.

## Out of scope

- Integrated browser, integrated terminal.
- A generic "router" agent.
- Calling a model API directly (D1).
- The app running git commands that change a repo (D4).
- Attachments (for now).

## Features

### Left pane

- **NAV-1** Several projects and conversations at once; conversations run
  concurrently. The left and right panes are resized by dragging their edge.
- **NAV-2** Conversations grouped by project/folder. Click a project to fold
  or unfold its conversations; the choice is kept. A project is added with
  its own button in the Projects header.
- **NAV-3** Pinned conversations sit at the top of the left pane, above the
  projects.
- **NAV-4** Unread indicator per conversation; the user can also mark one as
  unread.
- **NAV-5** Progress indicator per conversation.
- **NAV-6** PR status per conversation.
- **NAV-7** Selecting a conversation opens it in the main pane.
- **NAV-8** Rename, pin, mark as unread, archive and delete a conversation
  from its right-click menu in the left pane; search conversations.
- **NAV-9** Titles are generated automatically, as in Claude Code.
- **NAV-10** A + next to each project starts a conversation in it.

### Main pane

- **CONV-1** Send messages; see the agent's answer stream live.
- **CONV-2** Start a conversation directly with a chosen agent. The
  new-conversation form is a card like the mock's (labelled fields in two
  columns). A conversation always opens on its Flow tab (FLOW-4), also when
  clicked again in the left pane.
- **CONV-3** Choose the CLI per conversation: Claude Code, Codex, Copilot CLI.
- **CONV-4** Choose the model, the effort and the permission mode (default:
  auto). Models show their version (e.g. "Opus 5.5"); a picker never shows
  "default" but the value actually used.
- **CONV-5** Change CLI, model, effort or permission mid-conversation. When the
  CLI can't switch live, a new CLI session starts with the previous context.
- **CONV-6** Stop a running answer.
- **CONV-7** Go back to an earlier message: everything after it is dropped from
  the conversation.
- **CONV-8** Edit the last message (= go back to before it, send the new text).
- **CONV-9** Agent questions and permission requests appear as cards to answer
  with a click, as in Copilot and Claude Code.
- **CONV-10** The agent's suggested next message shows greyed in the empty
  message box; Tab writes it in.
- **CONV-11** Slash commands and skills from the message box. *(proposal: typing
  `/` opens a searchable list with each command's description)*
- **CONV-12** Long pasted text is folded into a "Pasted text · N lines" block,
  expandable, as in the Claude CLI.
- **CONV-14** An agent's plan, when its CLI keeps one (Claude Code's task
  list, Codex's plan): its steps above the message box — done, in progress,
  to do — and its progress on the agent's box in the Flow. Per agent
  session, while Leon runs.
- **CONV-13** Subagents: show how many were started and how many are still
  running, without their content.
- **CONV-14** The worktree / branch name of the conversation is visible; one
  click copies it. *(proposal: in the main-pane header)*

### Agents working together

An agent can start another agent **in a new agent session**: a separate CLI
conversation with its own history (a planner starting a coder, then a
reviewer…). These are not CLI subagents (those stay counts only, CONV-13). All
agent sessions that come from the user's first message belong to the **same
conversation**: one item in the left pane (D5).

- **FLOW-1** An agent starts another agent in a new session, with a prompt, and
  gets its answer back.
- **FLOW-2** An agent can send a follow-up to a session it started (e.g. the
  same coder fixes the reviewer's findings).
- **FLOW-3** Switch between the agent sessions of a conversation and talk to any
  of them directly.
- **FLOW-4** A flow diagram is the conversation's main tab: one box per agent
  session (plus a "You" box), arrows for the prompts sent and the reports
  returned, a highlighted arrow for a question waiting for you, and live status
  on each box. Click a box to open its session; click an arrow to read its
  prompts or reports. Each agent has its own colour. Agents that message each
  other in a loop (planner → coder → reviewer → planner) are placed on a
  circle, not on a long line. Arrows run as straight segments with
  rounded bends.
- **FLOW-5** Nothing hardcoded: the flow is built from what the agents actually
  do. A new agent that starts another agent shows up on its own.
- **FLOW-6** The diagram is a canvas: drag to pan, scroll or pinch to zoom, drag
  a box to rearrange it; "Reset view" fits everything, "Reset layout" puts the
  boxes back.

### Right pane

- **CTX-1** Information about the current conversation and its issue.
- **GIT-1** Uncommitted files of the conversation's working directory.
- **GIT-2** Diff of each uncommitted file. Changes and Branch list the files
  (with lines added and removed on the tab); clicking one opens a large drawer
  over the middle of the window with every file's diff, at the one clicked,
  unified or side by side.
- **GIT-3** Whole diff of the worktree against its starting branch/commit, as a
  PR shows it.
- **GIT-4** Auto-fold files whose path matches configured patterns (glob or
  regex).
- **GIT-5** Comment on lines of a diff: click a line's +, or drag it over
  several lines. Comments pile up as one pending review; "Send" posts them all
  as a single message to the conversation (file, lines, quoted code, comment).
- **GH-1** Status of the conversation's PR.

### GitHub

- **GH-2** My issues, at the top of the left pane (with GH-3), grouped by
  their GitHub Project status in the board's order (each status its own
  colour, each group foldable), sorted by priority, filterable by sprint.
  Clicking one opens its details in a drawer (fields, text, comments, link to
  GitHub). "Start conversation" opens the new-conversation form in the project
  whose repository it belongs to, with the issue in the message; the
  conversation remembers the issue, and the issue then offers "Open
  conversation".
- **GH-3** My open PRs, with their state icon and checks. Clicking one opens
  its details in a drawer: the checks summed up with the failing ones named,
  its description (GitHub's folded sections stay folded), the review threads
  still open, and its actions; one with a conversation has "Open
  conversation".
- **GH-4** Labels of issues and PRs.
- **GH-5** Request a Copilot review on a PR.
- **GH-6** Post a comment.
- **GH-7** Mark a pull request ready for review, or convert it back to a
  draft.

### Notifications

- **NOTIF-1** Desktop notification when an agent's answer is ready.

### Connectors

- **TOOLS-1** A Connectors view: one table, read-only, with a row per
  service (Slack, Gmail, Metabase…) working in at least one CLI and a column
  per CLI saying whether it works there (as a plugin, an MCP server or an
  account connector), to compare the CLIs; a search by name. Then Leon's shared MCP list, which
  every CLI's sessions get (add and remove servers there, I14).

### Settings

- **SETTINGS-1** A Settings view edits the config file: new-conversation
  defaults, diff auto-fold patterns, source folders. Saved at
  once into `config.toml`, keeping the rest of the file.

### Doctor

- **DOCTOR-1** A doctor view, like `flutter doctor`: Leon's version and setup
  (config, data folder, notifications…), and for each CLI and tool it uses
  whether it is installed, which version, whether it is logged in, and what
  to do when something is missing. Settings, Doctor and Connectors sit side by
  side at the bottom of the left pane.

### Usage

- **USE-1** Usage stats.

### Later

- **SCHED-1** Scheduled tasks — nice to have, last milestone.
- **GIT-6** Buttons for frequent git requests (commit, push, rebase…) that send
  the request to the agent as a message. Until then, the user asks the agent.

### Agents, skills, tools

- **EXT-1** Generic: a new user adds their own agent files and skills; no code
  change.
- **EXT-2** Agents, skills and tools (MCP servers, plugins) defined once, used by
  every CLI.
- **EXT-3** Today's standards, no invented formats.

### Platform and quality

- **PLAT-1** Builds and runs on macOS and Linux.
- **QUAL-1** Fast and very responsive — Zed is the reference.
- **QUAL-2** Small footprint.
- **QUAL-3** Clean, modern UI: one coherent colour theme, no emoji in the UI.
- **QUAL-4** The usual menus and shortcuts: Quit, Hide, Close, Minimize, an
  Edit menu (macOS dictation and emoji), ⌘N new conversation, ⌘, Settings,
  ⌘L message box, ⌘K search, ⌃Tab / ⌃⇧Tab next / previous conversation, Esc
  closes what is open on top. The left and right panes resize by dragging.
- **OSS-1** Open source.

## Layout *(proposal)*

```
┌──────────────────────┬────────────────────────────────────┬─────────────────────┐
│ PINNED               │ Fix login · pc-fix-login [copy]    │ Changes | Branch |  │
│   Release notes    • │ [Flow] Planner  Coder  Reviewer    │ PR | Info           │
│ PROJECT-A            │                                    │                     │
│   Fix login   (run)• │ transcript (streamed)              │ files               │
│   Add cache  (done)  │ subagents: 2 started, 1 running    │ ─────               │
│ PROJECT-B            │ ┌────────────────────────────────┐ │ diff + line comments│
│   Migrate   (error)• │ │ question card / suggestions    │ │                     │
│                      │ │ message…  [Pasted text · 80 l] │ │ [Send review (3)]   │
│ [+ New]              │ │ CLI▾ agent▾ model▾ effort▾ ... │ │                     │
└──────────────────────┴────────────────────────────────────┴─────────────────────┘
```

- **Status** (NAV-5): idle · running · needs input · error — shown with colour
  and shape, not emoji.
- **Unread** (NAV-4): set when a conversation not on screen finishes an answer,
  needs input or fails; cleared when shown.
- **PR badge** (NAV-6): draft · open + checks (pending / passing / failing) ·
  merged · closed. The PR is the one whose head is the conversation's branch,
  or one bound explicitly.
- **Tabs** (FLOW-3, FLOW-4): Flow first, then one tab per agent session. Flow is
  the default tab when the conversation has more than one agent session.
- **Flow diagram** (FLOW-4, FLOW-6): laid out left to right like a process
  flowchart (You → the agent you started → the agents it starts), right-angled
  arrows. One arrow per direction between two boxes, labelled with a count
  ("2 prompts"); the arrow panel lists each one with its time. Prompt arrows are
  solid, report arrows dashed, a question to you is amber, a prompt still being
  worked on is animated. The view refits on its own until you pan, zoom or move
  a box.
- **Branch diff** (GIT-3): `merge-base(base, HEAD)..HEAD`, like a PR;
  uncommitted changes stay in their own tab.
- **Auto-fold** (GIT-4): `diff.fold` patterns in the config, global and per
  project; no built-in patterns.

## Milestones *(proposal)*

| # | Milestone | Features |
|---|---|---|
| M0 | UI spike (P1), workspace, CI macOS + Linux, empty three-pane window, config, storage | PLAT-1, QUAL-* |
| M1 | Claude Code end to end: conversation with an agent, stream, stop, cards, resume | CONV-* |
| M2 | Codex, to prove the CLI abstraction early | CONV-3 |
| M3 | Left pane: groups, pins, unread, status, rename/archive/delete/search, notifications | NAV-*, NOTIF-1 |
| M4 | Agents working together: tabs, counts, flow view | FLOW-*, CONV-13 |
| M5 | Git: uncommitted, branch diff, auto-fold, review comments | GIT-1…5 |
| M6 | GitHub | NAV-6, GH-* |
| M7 | Shared agents, skills, tools | EXT-* |
| M8 | Copilot CLI | CONV-3 |
| M9 | Usage stats | USE-1 |
| M10 | Scheduled tasks, git buttons | SCHED-1, GIT-6 |

## Open

- Right pane: Changes · Branch · PR · Info enough? (CTX-1)
- When you talk directly to the coder, its answer stays with you; only answers
  to the planner's messages go back to the planner. *(proposal)*
- Queue the next message while an answer is running?
- Notifications (NOTIF-1): also when an agent asks a question?
- My issues / PRs: include review requests?
- Usage (USE-1): on subscriptions, plan limits (5 h / weekly windows) and tokens
  rather than cost? Where shown?
- GitHub (GH-6): inline review comments and replies? Review state? Other actions?
- Scheduled tasks: trigger, what runs, where results land, app open or not?
- Light and dark theme, or one?
- Command palette, several windows? (Settings screen and shortcuts: decided,
  SETTINGS-1, QUAL-4.)
- Resume sessions started in a terminal?
- Windows, Linux arm64, remote/SSH projects?
- GitHub Enterprise or several accounts?
- UI word for the CLI choice: "Tool" or "Backend"?
- Claude's plan mode as a permission choice?
- Any manual file editing in the app?
