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
  unfold its flows; the choice is kept; its folder icon shows open or
  closed. A project without flows (none, or all pinned or archived) shows
  folded. Drag a project among the others to move it: its line, folded,
  follows the mouse up and down in the pane's column, tilting a little with
  its speed like a card held where it was taken (`/` going down, `\` going
  up), and rocking back level when the mouse stops, while the others make
  room, and the order is kept across restarts; a new project comes last.
  Projects sit well apart, so each flow's project is plain. A project is added with
  its own button in the Projects header, and removed from Leon with its
  right-click menu, after a confirmation: its flows go with it, the
  folder and the CLIs' own sessions stay.
- **NAV-3** Pinned flows sit at the top of the left pane, above the
  projects.
- **NAV-4** Unread indicator per flow; the user can also mark one as
  unread, from its row's right-click menu, or mark one agent as unread, from
  its Flow box's right-click menu, which makes its flow unread too; or in one
  click with "Mark as unread" at the right of the flow's title bar: the agent
  whose tab is shown, else the flow. An answer that arrives while the user is not looking — another
  flow shown, its Flow or another agent's tab, or Leon behind
  another app — is unread until the user opens that agent's tab, comes back
  to Leon on it, or replies to it; the flow stays unread while one of its agents' answers is, and its
  Flow tab then shows a green dot. An agent
  that started others answers only once it has every report back: while one
  of them still works, or a report waits to reach it, its turns are not
  unread and do not notify.
- **NAV-5** Each flow's row says what it asks of the user: working
  (a loader, a little square jumping over its shadow, also on the agent's
  tab; on its Flow box, its colour going round it; "Working…"), needs you — an answer or a decision — (amber
  edge, "Needs you"), to review — done and not looked at yet — (green edge,
  "To review"), failed (red), or nothing. A folded project shows its most
  pressing colour. The line under its title says only that state, and its
  pull requests (NAV-6): no agent count, no time; a quiet flow is one line,
  its title, with its pull requests at the end of it.
- **NAV-6** The flow's PRs at the end of the line under its title: the PR
  icon and how many, in the colour of the most active one (open, then
  draft, merged, closed), in amber while an open one's checks run, or in
  red when an open one is blocked (a failed check, a conflict with its
  base), red first. On hover, the list of them, one line each (icon, title,
  number), each opening Leon's view of it.
- **NAV-7** Selecting a flow opens it in the main pane.
- **NAV-8** Rename, pin, mark as unread, archive and delete a flow from its
  right-click menu in the left pane; search flows. Rename edits the title in
  place, its text selected: typing replaces it, a click places the caret or
  selects text; Enter or a click elsewhere saves it. A message the user
  sends in an archived flow takes it out of the archive. A flow in the
  archive for more than a month (30 days) is deleted by Leon, looked for at
  launch and once a day while it runs; one archived before this rule counts
  from the update that brought it. A deleted flow leaves nothing of Leon's:
  its agents are stopped, even mid-turn, Leon's logs of them go (and at
  launch, any left from an earlier delete), and so do their Claude Code
  sessions (`~/.claude/projects`). Codex and Copilot CLI keep their
  sessions: they index them in databases of their own. Worktrees and
  branches stay: they can hold work not pushed.
- **NAV-9** Titles are generated automatically, as in Claude Code.
- **NAV-10** A + next to each project starts a flow in it.
- **NAV-11** A flow's row shows no issue: its issues are in the right
  pane's Info (CTX-1). Its title stays readable next to "To review" or
  "Needs you".
- **NAV-12** Back and forward through the places visited — a flow on its
  Flow tab or on a conversation's tab, or a page: To do, a GitHub list,
  Doctor, Connectors, Settings — like a browser: ⌘[ and ⌘], or a mouse's
  back and forward buttons. A place gone since (a deleted flow) is
  skipped.

### Main pane

- **CONV-1** Send messages; see the agent's answer stream live. A message
  being written stays with its conversation: shown another one, the message
  box shows that one's (or nothing), and coming back shows it again, its
  long pastes with it, until the app is closed.
- **CONV-2** Start a flow directly with a chosen agent. The new-flow page is
  a clean start page, as in GitHub Copilot's app: Leon's logo, then the
  message box with the agent, CLI, model, effort and permission in its row
  and Start, and under it the project (or, for another conversation in a
  flow, its folder) and the issue it is for. A flow always opens on its
  Flow tab (FLOW-4), also when clicked again in the left pane.
- **CONV-3** Choose the CLI per conversation: Claude Code, Codex, Copilot CLI.
- **CONV-4** Choose the model, the effort and the permission mode (default:
  auto). Models show their version (e.g. "Opus 5.5"); a picker never shows
  "default" but the value actually used.
- **CONV-5** Change CLI, model, effort or permission mid-conversation. When the
  CLI can't switch live, a new CLI session starts with the previous context.
- **CONV-6** Stop a running answer. A tool call left without a result when
  its turn ends (stopped, or the CLI never said) no longer shows as running:
  it reads "No result: its turn ended first".
- **CONV-7** Go back to an earlier message: everything after it is dropped from
  the conversation.
- **CONV-8** Edit the last message (= go back to before it, send the new text).
- **CONV-9** Agent questions and permission requests appear as cards to answer
  with a click, as in Copilot and Claude Code. A permission request shows
  what it would allow, whole, to judge it: the command, and the paths it
  reaches (e.g. Copilot CLI asking to access paths outside its trusted
  folders for a command it is about to run). Several asked at once (tools
  run side by side) are each asked, one card after the other, none lost.
- **CONV-10** The agent's suggested next message shows greyed in the empty
  message box; Tab writes it in.
- **CONV-11** Slash commands and skills from the message box: typing `/`
  opens a list with each command's description; Tab writes the first one in,
  the caret after it. The arguments a command takes (its `argument-hint`)
  show greyed after the caret until each is typed.
- **CONV-12** Long pasted text is folded into a "Pasted text · N lines" block,
  expandable, as in the Claude CLI. A long message in a conversation (more than 12
  lines or about 900 characters) shows its start, a few lines that never
  cut a code block, and "Show all".
- **CONV-14** An agent's plan, when its CLI keeps one (Claude Code's task
  list, Codex's plan): its steps above the message box — done, in progress,
  to do — and its progress on the agent's box in the Flow tab. Per
  conversation, while Leon runs.
- **CONV-15** Your messages stand out from the agents'. Once an answer
  follows your last message, a button next to Send scrolls back to it, to
  read the answer from its start; a conversation with an answer not read
  yet opens there.
- **CONV-16** While an agent works, the bottom of its conversation, just
  above the message box, shows live what it does this moment: each command
  or edit it runs and for how long, its running
  subagents, or "Thinking…"; under its plan when it keeps one. Below, and
  also while it does nothing itself, each agent it waits for — one it asked
  that has not answered yet — with that agent's state ("Waiting for coder 1 ·
  Working"), here and on its Flow box.
- **CONV-17** Agents keep working when Leon quits (I24): a conversation in a
  turn goes on, its CLI held by a small process of Leon's. Reopening Leon
  shows each one as it is — working, waiting for an answer, or done with
  its answer to read — the turn it was in replayed from its start. Leon's
  own agent tools (starting an agent, linking a PR) do not work while Leon
  is closed. Conversations between turns stop with Leon and resume on the
  next message.
- **CONV-18** Images in a message: paste one, or drop image files (PNG,
  JPEG, GIF, WebP) on the message box, in a conversation or on the new-flow
  page. Each reads "[Image #1]" where it was put, as the Claude CLI shows it
  (no thumbnail for now); removing that text removes the image. The images
  go to the agent with the message; they stay with the draft (CONV-1) until
  it is sent.
- **CONV-19** A message written while the agent is answering can be sent
  (Enter): it waits, shown as "Queued" above the message box with "Remove",
  and goes when the answer ends, before any other agent's message; a second
  one joins it.
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
  prompt — one of the project's agent files, or, as the user can, the CLI's
  own agent without an agent file — and gets its answer back — also from a turn that agent's CLI
  starts by itself (a background command it waited for has finished), or
  once it got an answer from an agent of its own — until the user writes
  to that conversation. Only then, or when started for the user, do its
  answers go to the user. The Flow tab's arrows follow the same rule. Or, when the user asks, it starts one
  for the user: that conversation's answers go to the user, who reviews and
  continues it, and nothing goes back to the agent that started it.
- **FLOW-2** An agent can send a follow-up to a conversation it started (e.g.
  the same coder fixes the reviewer's findings). Agents name the flow's
  conversations as the user sees them (planner, coder 1, coder 2…), so
  when the user names one, the agent reaches it by that name.
- **FLOW-3** Switch between the conversations of a flow and talk to any of
  them directly. A conversation's tab shows its name alone, not its CLI.
  Only the conversations the user talks with (wrote to, or started for
  them) and those they opened have a tab; a cross on the tab closes it,
  and the conversation stays in the flow and its diagram.
- **FLOW-4** A flow diagram is the flow's main tab: one box per conversation
  (plus a "You" box), each box saying the repository and branch its agent
  works in (CTX-2), a green "You" at the bottom right of the boxes of the
  conversations the user wrote to, so they stand out from those only agents
  talk to, arrows for the prompts sent and the reports returned, a
  highlighted arrow for a question waiting for you, and live status on each
  box, with "To review" or "Needs you" on the agent whose answer or question
  waits for you; while an agent works, its colour goes round its box's
  border, starting from the band at its top, in place of the
  loader. What needs you shows with the
  left pane's icon (a green check for "To review") and coloured line along
  the box's left side, exactly as the left pane shows them. Click a box to open its
  conversation; click an arrow to read its prompts or reports. Each agent has
  its own colour, the same in the right pane. Laid out by forces, after the
  "force" mode of CS Academy's graph editor: boxes push each other away, the
  arrows between them pull like springs, and boxes never overlap. So the boxes a conversation talks to
  spread all around it, every arrow about the same length (measured between
  the boxes' borders, as boxes are wide). The forces are CS Academy's own: their
  rules and strengths, scaled to Leon's boxes, no momentum, boxes pushing
  only the ones near them, without its pull toward the middle: the diagram
  stays where its boxes are put. It starts with each box's linked boxes spread
  evenly around it, each agent's conversations side by side, and the same
  flow always gets the same layout. Readability comes first; an arrow may
  cross another. Arrows are straight lines from box to box; a box sitting on
  one pushes it into a curve round its other side, as CS Academy bends an
  edge a node sits on. Those between the
  same two boxes (prompts one way, reports back) run side by side as one
  bundle under one small label, put where it covers no box and no other
  label; arrows of one kind going both ways (two agents that asked each
  other) share one line with a head at each end. The label shows only how
  many messages each line carries, in the line's colour, which the legend
  explains. A
  bar sums up the whole flow: its conversations, how many work, their
  messages, tool calls and usage.
- **FLOW-5** Nothing hardcoded: the flow diagram is built from what the agents
  actually do. A new agent that starts another agent shows up on its own.
- **FLOW-6** The diagram is a canvas: drag to pan, scroll or pinch to zoom, drag
  a box: the forces keep acting, live and soft, so a box let go bounces back
  to its place, or finds a new one when dragged far enough, and the others
  follow it loosely as in CS Academy's force mode; "Reset view" fits everything,
  "Reset layout" goes back to where the forces first settled it.
- **FLOW-7** Start a new conversation in an existing flow: "+ New
  conversation" at the top right of the Flow tab opens the form (one of the flow's folders, agent, CLI, model, effort,
  permission, first message); the new conversation gets its own tab and
  message box, and the user is the one who started it on the flow diagram.

### Right pane

- **CTX-1** The right pane's tabs are Info, Changes and Branch.
  Info is the whole flow's, whatever its repositories and worktrees: under
  "Pull requests", every pull request of the flow by state only (open,
  then draft, merged, closed), so none is missed; under "Issues", every
  issue it is linked to (each opening Leon's view of it, GitHub from
  there); then its to-dos (TODO-2). They stay up to date: opening a pull request's details updates
  it everywhere, and the flow looked at is read again every 30 s while one
  of its checks runs. What an agent is doing
  shows in its conversation (CONV-16), not here.
  Changes and Branch show one of the flow's worktrees at a time (CTX-2),
  named on one line under the tabs, as Zed's two pickers: the repository,
  picked there when the flow works in several, then its worktree by its
  branch name, picked there when that repository has several (a
  repository's own checkout, not a worktree of it, reads "main ·
  <branch>"); then a button copying that branch.
  The right pane's tabs
  are Info, Changes, Branch; a flow opens it on Info, and a tab picked by
  hand is kept per flow. The right pane stays narrow: a pull
  request opens in the details drawer, not in a tab of its own. A pull
  request or an issue shows the same wherever it is listed (Info, GH-2,
  GH-3, NAV-6): its icon in its state's colour (an issue's in its board
  column's), its title, its number at the end of the line (no state word,
  no age); under it its repository,
  and for a pull request which branch it merges into which ("fix-retry →
  dev", the destination brighter), the lines it adds and removes, its CI
  as one indicator in its state's colour — passing; running, how many jobs
  are done ("3/13 done"); failed, a red cross and the failing jobs by name,
  each opening its run on GitHub; skipped — its conflicts, then the
  labels. No coloured bar along its side. In
  Info a merged or closed pull request folds into one small line: its
  icon, title and number. Leon shows no time since anything happened (last
  opened, done, checked…): only how long something running has run.
- **CTX-2** One flow can edit several repositories, in several
  worktrees, with several PRs, linked to several issues, and Leon follows
  what the agents do: a folder joins the flow when an agent edits a
  file in it, tells Leon it moved there, or is started in it; each folder's branch brings its
  PR; each PR brings the issues it closes, after the issue the flow
  was started for. An agent also links to the flow each issue or PR it
  creates or works on, and the flow keeps them all. Several agents can
  work in several folders at once: on an agent's tab the right pane shows
  that agent's folder unless another is picked on that tab. The right pane
  offers only the worktrees an agent works in now. A repository's own
  checkout (the main checkout worktrees start from) shows, with its pull
  request, only once an agent works in it — changes a file there or says it
  works there — on any branch; an agent merely started there, or working in
  a worktree of it, leaves it hidden, so a diff or PR of other work never
  shows.
- **CTX-3** Leon tells every agent conversation, in every CLI, to keep it up to
  date with its tools each time it is needed (moving to another folder,
  naming the flow, linking an issue or PR it creates or works on), so the
  app always shows the current work.
- **GIT-1** Uncommitted files of the flow's folder (CTX-2).
- **GIT-2** Diff of each uncommitted file, code in its language's colours
  as in Zed (I21). Changes and Branch list the files
  (with lines added and removed on the tab); clicking one opens a large drawer
  over the middle of the window with every file's diff, at the one clicked,
  and the same list of files down its side to go to another, unified or
  side by side; side by side, the divider between the sides is dragged
  left or right.
- **GIT-3** Whole diff of the worktree against its starting branch/commit, as a
  PR shows it. That base is fetched from the remote in the background when
  the flow is opened, at most every five minutes.
- **GIT-4** Auto-fold files whose path matches configured patterns (glob or
  regex).
- **GIT-5** Comment on lines of a diff: click a line's +, or drag it over
  several lines; the lines stay highlighted while the comment is written,
  and side by side the comment box sits under their side. Comments pile up
  as one pending review; "Send" posts them all
  as a single message to the conversation (file, lines, quoted code, comment).
- **GIT-7** Open in the user's editor: a diff's file header ("Open in Zed")
  opens its worktree in the editor with that file open; "Open in Zed" at the
  end of the right pane's worktree line (its icon alone when the pane is
  narrow) and "Open this worktree in Zed" in the diff drawer's header open
  the worktree. A pull request's diff, from GitHub, is
  read-only and says so: comments for an agent go on the flow's own diffs. The editor is chosen in Settings among
  those installed (VS Code, Zed), the first one found by default.
- **GH-1** Status of each of the flow's PRs.

### To-dos

- **TODO-1** A flow keeps to-dos: later steps for the user, each waiting
  for something — not an agent's own plan while it works (CONV-14). A to-do
  has a when (what it waits for: a deploy, a review, someone's answer, a
  date) and a what (the action), and reads "<when> → <what>": the when, after
  an amber hourglass, as wide as it reads up to 40 % of the row, the what
  beside it, each wrapping on several lines when long.
  The user adds one in the flow's Info tab, the when optional; an agent
  adds one with Leon's `add_todo` tool, which needs both, and refuses a
  to-do that waits for nothing (a step the user can take now belongs in the
  agent's answer); it is told a to-do is not for a ticket's routine
  (merging, deploying, reviewing, checking it worked) either. An agent
  reads the flow's to-dos with `list_todos`, open then done, each with the
  agent that added it.
  The user ticks one with its box: it is done at once; a done one opens
  again with its box. The small red bin its row shows on hover removes it
  for good. Either way, a note at the window's bottom right offers "Undo"
  for a few seconds, which puts the to-do back as it was. A double click on
  a to-do's when or what edits it in place, its text where it was: Enter or
  leaving the box saves it, Esc gives up.
- **TODO-2** Info (CTX-1) shows, under the flow's pull requests and issues,
  its open to-dos under "To do", with a box to add one; the done ones fold
  under "N done". A to-do's what shows as markdown, its line breaks kept.
  The "To do" heading folds the whole section, in every flow, kept across
  restarts; folded, it still says how many are open.
- **TODO-3** A button at the top right of the main pane, just left of the
  right pane, says how many to-dos are open in every flow; it opens the To
  do page in the main pane: one foldable group per flow with open to-dos
  (the flow's title, which opens it, and project, marked in the project's
  colour), its open to-dos, each marked done there, and its done ones
  folded. The user drags a group by its header to move it among the others, as a
  project in the left pane (NAV-2); the order is kept across restarts, and
  a flow given its first open to-do comes last.
- **TODO-4** A flow's row in the left pane shows how many of its to-dos
  are open.

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
  issue, and the issue then also offers "Open flow". "Start flow" is always
  offered, on a row's hover and in its drawer, even when a flow was already
  started for the issue: each new flow is linked to it too. When the lists
  cannot be read (no GitHub CLI, not logged in), the left pane says nothing:
  the list's page says why, and Doctor shows the problem.
- **GH-3** My open PRs, grouped by repository (each group foldable), with
  their state icon and checks. Clicking one opens its details in a drawer,
  laid out like its GitHub page: title and state pill, who wants to merge
  which branch into which with its labels on the right of that line, while
  it is open and GitHub says its branch conflicts with its base the box
  "This branch has conflicts that must be resolved" (also "Conflicts with
  <base>" on its row wherever it is listed, CTX-1), the checks box (how they went, the failing ones
  named), its description (GitHub's folded sections stay folded) and each
  review thread still open with the code it is on, people as coloured
  avatars, a box to comment. "Files changed" opens its changes over it, in
  the large drawer, read from GitHub so they are exactly what GitHub's own
  "Files changed" shows, whatever is checked out locally; read-only. Its actions (Copilot review, then ready or
  draft) sit left of "Copy link", which puts its
  address on the clipboard, and "Open on GitHub".
- **GH-4** Labels of issues and PRs.
- **GH-5** Request a Copilot review on a PR.
- **GH-6** Post a comment.
- **GH-7** Mark a pull request ready for review, or convert it back to a
  draft.
- **GH-8** Answer a review thread and resolve it, from Leon's view of the PR,
  or both at once ("Reply and resolve"; a failed answer resolves nothing).
  "Resolve as fixed" answers "Fixed" first, then resolves the thread.
  An action's button (GH-5..8) shows it under way the moment it is clicked
  ("Converting to draft…", "Posting…"), a second click does nothing, and its
  notice says how it went once GitHub answers.
- **GH-9** Every reference to an issue or a pull request — in the left pane,
  the right pane, the details drawer, or a GitHub link in a message — opens
  Leon's view of it first, GitHub from there.

### Notifications

- **NOTIF-1** Desktop notification, with a short sound, when an agent's answer
  is ready, and when an agent asks a question.

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
- **SETTINGS-2** The app's zoom: 100 (default), 125 or 150 %. Text, icons,
  spacing and widths grow together, so the conversation column and the
  panes keep their proportions; the flow diagram keeps its own zoom. A
  page's column (a conversation, Settings, the GitHub lists, Doctor,
  Connectors) also grows with the window: half the room a wider pane
  leaves goes to it.
- **SETTINGS-4** The editor "Open in …" uses (GIT-7), among those Leon
  finds installed.
- **SETTINGS-3** What a flow opens on when picked in the left pane: its
  Flow diagram (default) or its main conversation, the one the user started
  it with. A flow of one conversation opens on it either way: a diagram of
  one box says nothing. The Flow tab stays either way.

### Doctor

- **DOCTOR-1** A doctor view, like `flutter doctor`: Leon's version and setup
  (config, data folder, notifications…), and for each CLI and tool it uses
  whether it is installed, which version, whether it is logged in, and what
  to do when something is missing. A coding CLI not installed, when another
  is, says "Not installed" and that it is optional, neither green nor a
  warning nor a problem; no coding CLI at all is a problem, and so is a
  missing git or GitHub CLI (both needed, for now). Doctor's icon in the left
  pane carries a dot only for a warning (amber) or a problem (red), none
  when all works. Its first check runs once
  Leon's own tools listen, so it is right at launch. Settings, Doctor and Connectors sit side by
  side at the bottom of the left pane.
- **DOCTOR-2** Usage limits, first in Doctor: for each CLI that said how
  much of the user's plan is used, one bar per window (5h, week…), its
  share — amber from 75 %, red from 90 % — and when it starts again, with
  when the CLI said it ("said 5m ago"). What a CLI said last is kept across
  restarts until it says again: Claude Code says it during a turn, Codex
  also when Doctor runs (at launch, "Run again"); a window past its reset
  shows empty. Copilot CLI does not say it (its plan budget and limits show
  only in its own `/usage`).
- **DOCTOR-3** Updates: Leon checks for a newer release at launch and once a
  day. Doctor then shows that it is available (which version, which one runs,
  a link to its release notes), and its icon in the left pane carries a blue
  dot (a warning or a problem keeps its own colour). On macOS, "Update"
  downloads it, checks it is Leon's own (same checksum as GitHub gives, signed
  and notarized by the same developer), puts it in place of the Leon that
  runs, then offers "Restart Leon"; agents keep working through the restart.
  Elsewhere, only the link. Without the GitHub CLI logged in, Doctor says it
  cannot check.

### Usage

- **USE-1** Usage stats: plan limits stay as they are (DOCTOR-2); for a CLI
  not on a subscription, its cost and credit usage: Claude Code's cost under
  each answer only when it runs on an API key (on a subscription it is only
  what the API would bill); the credits Codex's account has left, in
  Doctor's usage card, when it has some. Copilot CLI runs on a subscription.

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
- **QUAL-3** Clean, modern UI: one coherent colour theme, no emoji in the UI;
  a repository shows by its name, without its owner (`api#12`, not
  `acme/api#12`).
- **QUAL-4** The usual menus and shortcuts: Quit, Hide, Close, Minimize, an
  Edit menu (macOS dictation and emoji), ⌘N new flow, ⌘, Settings,
  ⌘L or Tab message box, ⌘K or / search (Tab and / only outside a text
  box, where they type as usual), ⌃Tab / ⌃⇧Tab next / previous flow, ⌘[ / ⌘]
  back / forward (NAV-12), Esc
  closes what is open on top. The left and right panes resize by dragging.
- **QUAL-5** Copying from the interface (a code block, the worktree's branch,
  a pull request's link) shows "Copied" for a moment just above where it was
  clicked.
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
- **Tabs** (FLOW-3, FLOW-4): Flow first, then one tab per conversation the
  user talks with or opened: the one shown, then the others from the one
  opened last, then those never opened in start order; opening a tab brings
  the row back to its start. The cross shows on the tab shown and on a
  hovered one; closing the tab shown shows the Flow tab. A closed tab stays
  closed across restarts, and comes back when its conversation is opened
  again (its box, a notification).
  A flow opens on its Flow tab, or on its main conversation (SETTINGS-3).
- **Flow diagram** (FLOW-4, FLOW-6): laid out by forces from columns of who
  started whom; straight arrows, bundled by pair of boxes. One line per kind between two boxes, both ways sharing it,
  labelled with its count ("2"); the arrow panel lists each message with its
  time and, on a line going both ways, who sent it. Prompt arrows are
  solid, report arrows dashed, a question to you is amber, a prompt still being
  worked on is animated. The view refits on its own until you pan, zoom or move
  a box.
- **Branch diff** (GIT-3): `merge-base(base, HEAD)..HEAD`, like a PR;
  uncommitted changes stay in their own tab. The base (the PR's, else the
  remote's default branch) is fetched with `git fetch origin <base>` when the
  flow is opened or its Changes, Branch or folder is clicked, at most every
  five minutes per folder (I22).
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

- Scheduled tasks: trigger, what runs, where results land, app open or not?
  *(still to define)*
- Remote projects over SSH, as in Zed *(maybe, later)*.
