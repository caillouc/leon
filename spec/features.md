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

- **NAV-1** Several projects and flows at once; flows run concurrently. The
  left and right panes are resized by dragging their edge.
  The right pane closes with its panel button, or by dragging its edge far
  enough; the same button in the flow's header opens it again.
- **NAV-2** Flows grouped by project/folder. Click a project to fold or
  unfold its flows; the choice is kept. A project is added with
  its own button in the Projects header, and removed from Leon with its
  right-click menu, after a confirmation: its flows go with it, the
  folder and the CLIs' own sessions stay.
- **NAV-3** Pinned flows sit at the top of the left pane, above the
  projects.
- **NAV-4** Unread indicator per flow; the user can also mark one as
  unread. An answer that arrives while the user is not looking — another
  flow shown, its Flow or another agent's tab, or Leon behind
  another app — is unread until the user opens that agent's tab or replies to
  it; the flow stays unread while one of its agents' answers is.
- **NAV-5** Each flow's row says what it asks of the user: working
  (a turning loader, also on the agent's tab and Flow box, "Working…"), needs you — an answer or a decision — (amber
  edge, "Needs you"), to review — done and not looked at yet — (green edge,
  "To review"), failed (red), or nothing. A folded project shows its most
  pressing colour.
- **NAV-6** The status of each of the flow's PRs.
- **NAV-7** Selecting a flow opens it in the main pane.
- **NAV-8** Rename, pin, mark as unread, archive and delete a flow from its
  right-click menu in the left pane; search flows.
- **NAV-9** Titles are generated automatically, as in Claude Code.
- **NAV-10** A + next to each project starts a flow in it.
- **NAV-11** A flow linked to issues (CTX-2) shows the first issue's
  number in its row, and how many more; clicking it opens Leon's view of the
  issue (GH-2).

### Main pane

- **CONV-1** Send messages; see the agent's answer stream live.
- **CONV-2** Start a flow directly with a chosen agent. The new-flow form is
  a card like the mock's (labelled fields in two columns). A flow always
  opens on its Flow tab (FLOW-4), also when clicked again in the left pane.
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
- **CONV-11** Slash commands and skills from the message box: typing `/`
  opens a list with each command's description; Tab writes the first one in,
  the caret after it. The arguments a command takes (its `argument-hint`)
  show greyed after the caret until each is typed.
- **CONV-12** Long pasted text is folded into a "Pasted text · N lines" block,
  expandable, as in the Claude CLI.
- **CONV-14** An agent's plan, when its CLI keeps one (Claude Code's task
  list, Codex's plan): its steps above the message box — done, in progress,
  to do — and its progress on the agent's box in the Flow tab. Per
  conversation, while Leon runs.
- **CONV-13** Subagents: show how many were started and how many are still
  running, without their content.
- **CONV-14** The worktree / branch name of the flow is visible (the
  folder picked in the right pane, CTX-2); one click copies it. *(proposal: in the main-pane header)*

### Agents working together

An agent can start another agent **in a new conversation**: a separate CLI
conversation with its own history (a planner starting a coder, then a
reviewer…). These are not CLI subagents (those stay counts only, CONV-13). All
conversations that come from the user's first message belong to the **same
flow**: one item in the left pane (D5).

- **FLOW-1** An agent starts another agent in a new conversation, with a
  prompt, and gets its answer back.
- **FLOW-2** An agent can send a follow-up to a conversation it started (e.g.
  the same coder fixes the reviewer's findings).
- **FLOW-3** Switch between the conversations of a flow and talk to any of
  them directly.
- **FLOW-4** A flow diagram is the flow's main tab: one box per conversation
  (plus a "You" box), arrows for the prompts sent and the reports returned, a
  highlighted arrow for a question waiting for you, and live status on each
  box, with "To review" or "Needs you" on the agent whose answer or question
  waits for you, as the left pane says it. Click a box to open its
  conversation; click an arrow to read its prompts or reports. Each agent has
  its own colour. The boxes fill the space rather than a long line: one region
  per agent, the conversations of the same agent together in it, regions in the
  order the agents appeared on rows that snake, so a loop (planner → coder →
  reviewer → planner) reads as a loop. Arrows run as straight segments with
  rounded bends.
- **FLOW-5** Nothing hardcoded: the flow diagram is built from what the agents
  actually do. A new agent that starts another agent shows up on its own.
- **FLOW-6** The diagram is a canvas: drag to pan, scroll or pinch to zoom, drag
  a box to rearrange it; "Reset view" fits everything, "Reset layout" puts the
  boxes back.
- **FLOW-7** Start a new conversation in an existing flow: "+ New" after its
  tabs opens the form (one of the flow's folders, agent, CLI, model, effort,
  permission, first message); the new conversation gets its own tab and
  message box, and the user is the one who started it on the flow diagram.

### Right pane

- **CTX-1** Information about the current flow, its issue and pull
  request (each opening Leon's view of it, GitHub from there), and
  first what each agent is doing now: working on which commands and for how
  long, how many subagents run, the answer it waits for, or idle. A flow
  opens the right pane on Info until it works in its own worktree, then on
  Changes; a tab picked by hand is kept per flow.
- **CTX-2** One flow can edit several repositories, in several
  worktrees, with several PRs, linked to several issues, and Leon follows
  what the agents do: a folder joins the flow when an agent edits a
  file in it or tells Leon it moved there; each folder's branch brings its
  PR; each PR brings the issues it closes, after the issue the flow
  was started for. Info lists them all. Changes, Branch and PR show one
  folder at a time, picked from a row of chips (repository · branch · PR
  state) when there are several.
- **CTX-3** Leon tells every agent conversation, in every CLI, to keep it up to
  date with its tools each time it is needed (moving to another folder,
  naming the flow), so the app always shows the current work.
- **GIT-1** Uncommitted files of the flow's folder (CTX-2).
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
- **GH-1** Status of each of the flow's PRs.

### GitHub

- **GH-2** My issues, at the top of the left pane (with GH-3), grouped by
  their GitHub Project status in the board's order (each status its own
  colour, never shared by two statuses; each group foldable, kept folded
  across restarts), sorted by priority (each priority in its
  board colour), filterable by sprint: the current sprint by default, each
  sprint shown with its dates.
  Clicking one opens its details in a drawer (fields, text, comments, link to
  GitHub). "Start flow" opens the new-flow form in the project whose
  repository it belongs to, the message starting with the issue
  (`owner/repo#12` and a blank line, the caret on it); the flow remembers the
  issue, and the issue then offers "Open flow".
- **GH-3** My open PRs, grouped by repository (each group foldable), with
  their state icon and checks. Clicking one opens its details in a drawer,
  laid out like its GitHub page: title and state pill, who wants to merge
  which branch into which, the checks box (how they went, the failing ones
  named), its description (GitHub's folded sections stay folded) and each
  review thread still open with the code it is on, people as coloured
  avatars, a box to comment. Its actions (ready or draft, Copilot review,
  "Open flow" when it has one) sit left of "Open on GitHub".
- **GH-4** Labels of issues and PRs.
- **GH-5** Request a Copilot review on a PR.
- **GH-6** Post a comment.
- **GH-7** Mark a pull request ready for review, or convert it back to a
  draft.
- **GH-8** Answer a review thread and resolve it, from Leon's view of the PR.

### Notifications

- **NOTIF-1** Desktop notification when an agent's answer is ready.

### Connectors

- **TOOLS-1** A Connectors view: one table, read-only, with a row per
  service (Slack, Gmail, Metabase…) working in at least one CLI and a column
  per CLI saying whether it works there (as a plugin, an MCP server or an
  account connector), to compare the CLIs; a search by name. Only what the
  user installed or connected is listed, not what a CLI ships or turns on by
  itself. The CLIs are asked at the same time; each column fills in when its
  CLI answers and says it is still checking until then. Then Leon's shared MCP list, which
  every CLI's conversations get (add and remove servers there, I14).

### Settings

- **SETTINGS-1** A Settings view edits the config file: new-flow
  defaults (agent — picked when the project has it —, CLI, model, effort,
  permission), diff auto-fold patterns, the folders of agents and skills (the
  ones Leon finds itself and the ones the user adds, each with how many
  agents and skills it holds), the order in which CLIs are tried for an
  agent's model (EXT-6), changed by dragging the CLIs' boxes. Saved at
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
- **EXT-4** Generic agent files, not tied to one CLI: `~/.agents/agents/` and a
  project's `.agents/agents/`, one Markdown file per agent (frontmatter
  `name`, `description`, optional `model` and `tools`; the body is its
  instructions). Leon also reads `.claude/agents/` and `.github/agents/`;
  on a name clash `.agents/` wins. Leon bridges each file to the CLI that runs
  it; how is documented for users.
- **EXT-5** Generic skills: `~/.agents/skills/<name>.md` and a project's
  `.agents/skills/`. Every CLI sees them and loads one only when the agent
  needs it (names and descriptions first). An agent names a skill in its text
  if it must use it; there is no skills field.
- **EXT-6** An agent file's `model` picks the CLI: the first CLI that offers
  that model, in an order set in Settings (default Claude Code, Codex,
  Copilot CLI). Also for an agent another agent starts. Without a `model`,
  the form's or the calling agent's CLI and model.
- **EXT-7** An agent file's `tools` limits what the agent can do, in GitHub's
  custom-agent words (`read`, `search`, `edit`, `execute`, `web`, `agent`,
  `todo`; Claude Code's tool names are understood too). Without `tools`, the
  agent has every tool its CLI has; MCP tools are never limited.
- **EXT-8** Shared instructions: `~/.agents/AGENTS.md` goes to every agent, in
  every CLI, like a `CLAUDE.md` for all of them. A project's own `AGENTS.md`
  stays with the CLIs, which read it themselves.

### Platform and quality

- **PLAT-1** Builds and runs on macOS and Linux.
- **QUAL-1** Fast and very responsive — Zed is the reference.
- **QUAL-2** Small footprint.
- **QUAL-3** Clean, modern UI: one coherent colour theme, no emoji in the UI.
- **QUAL-4** The usual menus and shortcuts: Quit, Hide, Close, Minimize, an
  Edit menu (macOS dictation and emoji), ⌘N new flow, ⌘, Settings,
  ⌘L message box, ⌘K search, ⌃Tab / ⌃⇧Tab next / previous flow, Esc
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
- **Unread** (NAV-4): set when a flow whose messages are not on screen —
  not shown, shown on its Flow tab, or Leon's window not the active one —
  finishes an answer, needs input or fails; cleared when the user opens one
  of its agents' tabs or replies in it.
- **PR badge** (NAV-6): draft · open + checks (pending / passing / failing) ·
  merged · closed. One per folder of the flow (CTX-2): the PR whose
  head is that folder's branch, or one bound explicitly.
- **Tabs** (FLOW-3, FLOW-4): Flow first, then one tab per conversation. Flow is
  the default tab when the flow has more than one conversation.
- **Flow diagram** (FLOW-4, FLOW-6): laid out on a grid of regions (You, then
  one region per agent in the order it appeared, the rows snaking), right-angled
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
| M4 | Agents working together: tabs, counts, flow diagram | FLOW-*, CONV-13 |
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
