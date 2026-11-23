# Design

How Leon looks and moves. Every screen follows this; a change to the look
changes this file in the same change. Values are the code's: the theme in
`crates/leon-ui/src/theme.rs`, shared pieces in `crates/leon-ui/src/widgets.rs`.
A new colour, size, shape or component not listed here is asked to the
maintainer first.

## Principles

- **Calm and dense.** One dark theme (QUAL-3). Text and lines carry the
  screen; colour is kept for what means something.
- **Colour means a state.** Each meaning has one colour, everywhere (below).
  Never a colour for decoration; never red for anything but an error.
- **Shape backs colour.** A state reads without colour too: its icon or
  shape differs. No emoji.
- **One way per thing.** A button, a card, a pill, a status mark look the
  same on every screen. Reuse the shared function; add one when a pattern
  appears twice.
- **Sizes grow with the zoom.** Every size goes through `theme::rem(px)`,
  the size in pixels at 100 % (SETTINGS-2). Only the flow diagram (its own
  zoom) and GPUI's pixel-only APIs use `px`.

## Layout

- Three panes: **left** 268 (200–480), **main** at least 420, **right** 440
  (320–900; dragged under 200 it closes). Panes resize by dragging their
  edge.
- Every pane's top row is **48** high (`HEADER_HEIGHT`) with a hairline
  under it, so the dividers line up. A page's header is `pane_header`:
  title 15 semibold, a muted subtitle, actions on the right.
- **Page columns** are centred with `widgets::centered(base, …)`: `base`
  wide plus half the room a wider pane leaves; all of a narrower one.
  Bases: conversation and its message box 860 (`theme::COLUMN`), Settings
  760, Doctor 820, Connectors 820, GitHub lists 880, the new-flow page 680.
- **Start pages** (a new flow, after GitHub Copilot's app): vertically
  centred, Leon's bicorne (the app icon's hat alone, `leon-hat.png`, 160
  wide, 64 above the box), then the message box, then quiet pickers and the agent's
  description under it. No labelled form.
- **The right pane**: its tabs at the header's height (48), the close
  button at their right. Info is the whole flow's: PR rows under a
  "Pull requests" section label, then issue rows under "Issues", no box. On Changes and Branch, one line (36) under the tabs names
  the worktree they show, as Zed's two pickers — folder icon, the
  repository (a title picker when the flow works in several), its worktree
  by branch (a branch picker, mono and muted, when that repository has
  several: branch names only, "main · <branch>" for the repository's own
  checkout), a copy button for the branch.
- **A diff drawer** lists its files down its left side (280, `panel`), the
  same rows as the Changes and Branch tabs; the diffs fill the rest.
- **Drawers** (a PR or issue, a diff) open over everything: 84 % of the
  window wide, at most 1400, 90 % high, on a dimmed backdrop (`scrim`).
  Close button top right; a click outside or Esc closes them.

## Spacing and shape

- GPUI's scale, in steps of 4: `gap_1` 4, `gap_2` 8, `gap_3` 12, `px_3` 12,
  `px_4` 16. Rows: 5–8 vertical padding; cards: 12–16 inside, 12 between.
  In the left pane, 16 above each project, so its flows read as its own:
  a project's name is a heading (12.5 semibold, `fg_muted`, its folder
  icon), its flows set in 14 under it, their titles bright; a pinned flow
  sits at the pane's edge.
- Radii: 4 small controls and chips (`rounded(rem(4))`), 6 buttons and rows
  (`rounded_md`), 8 cards (`rounded_lg`), 12 drawers and Doctor cards
  (`rounded_xl`), full for pills and dots.
- Hairlines are 1 (`border`); a card's or menu's edge `border_strong`. A
  coloured edge on the left (2–3) marks what needs the user, and only
  that: a PR, an issue or an agent never carries a permanent colour bar;
  its icon or name in its colour says it.

## Typography

- The system font; **mono** (`theme::MONO`: Menlo, DejaVu Sans Mono) for
  code, diffs, paths, branches, references (`api#12`) and model ids.
- Sizes, by role:

| Size | Role |
|---|---|
| 21 / 17 | A PR's title / an issue's title in their view |
| 15 | Pane title (`pane_header`), the flow's title, a form's title |
| 14 | Card and drawer titles |
| 13–13.5 | Body text, group headings |
| 12.5 | Rows, secondary body, notes in cards |
| 12 | Buttons, controls, meta lines |
| 11.5 | Row meta (under a flow's title), small headings |
| 11 | Chips, counts, mono details |
| 10.5 | Section labels (uppercase, faint: `widgets::label`), pills |

- Weights: semibold for titles and for what needs the user; medium for a
  name to stand out (an agent, a question); regular otherwise.
- Text is cut with an ellipsis on one line, never wrapped in rows.

## Colour

The palette is GitHub's Primer dark, as GitHub Copilot's app uses it
(GitHub theme, v1.4.3): its names in brackets. Tokens (`theme.rs`), never a
hex elsewhere except GitHub's own label colours and the agent palette:

| Token | Hex | GitHub | Use |
|---|---|---|---|
| `window` | 0d1117 | bgColor-default | Window, main pane, cards in a side pane |
| `panel` | 151b23 | bgColor-muted | Side panes, Doctor cards |
| `surface` | 212830 | control-bgColor-rest | Raised: inputs, the message box, buttons |
| `surface_2` | 262c36 | control-bgColor-hover | Hover, chips |
| `overlay` | 010409 | overlay-bgColor | Menus, pickers' lists, hover lists |
| `border` | 3d444d at 60 % | borderColor-default | Hairlines |
| `border_strong` | 3d444d | button border | Cards, menus, buttons |
| `fg` / `fg_muted` / `fg_faint` | f0f6fc / 9198a1 / 656c76 | fgColor-default / muted / disabled | Text: main / secondary / hints, labels, idle |
| `accent` | 4493f8 | fgColor-accent | Selection, primary button, links, **working** |
| `accent_soft` | 388bfd at 15 % | bgColor-accent-muted | Selected row, chosen segment |
| `warn` | d29922 | fgColor-attention | **Needs you**: a question, a decision; checks running; 75 % used |
| `ok` | 42a353 | fgColor-success (3fb950), a shade calmer (asked) | **To review** / done, success, open PR, "You" |
| `err` | f85149 | fgColor-danger | **Error**: failed, failing check, closed PR, conflicts, 90 % used |
| `merged` | ab7df8 | fgColor-done | Merged PR |
| `add_bg` / `del_bg` (+ gutters) | 2ea043 at 20 % / f85149 at 18 % | diffBlob, a little stronger | Diff lines, kept stronger than GitHub's (asked) |
| `scrim` | 010409 at 60 % | overlay black | Behind a drawer |

- Code in diffs takes GitHub's code colours ("prettylights": keyword ff7b72,
  string a5d6ff, constant and number 79c0ff, function d2a8ff, type ffa657,
  tag 7ee787, comment 9198a1), in `syntax.rs`.
- A tinted box is its colour at 12 % with an edge at 35 % (a problem's fix
  in Doctor); a pill is its colour at 18 % with the colour as text.
- **Agents** get a colour from `theme::agent_color` (8 of GitHub's label
  colours, in start order),
  the same in the diagram and tabs.
- **Board columns** get `theme::status_colors`: backlog grey, to do blue,
  in progress amber, review green, done purple; never red.
- **GitHub labels** keep GitHub's colours (red shown as a deeper orange).

## Status

One vocabulary for an agent, a flow and a check:

| State | Mark | Colour | Row edge | Pill / text |
|---|---|---|---|---|
| Working | the jumping square (loader) | accent | — | "Working…" |
| Needs you | message icon; filled square | warn | amber | "Needs you" |
| To review | square check | ok | green | "To review" |
| Error | square cross; filled square | err | red | "Stopped with an error" |
| Idle | outlined square, faint | fg_faint | — | when it was last active |

- **Status marks are small squares**, radius 2: a conversation's
  (`widgets::status_dot`, 9), any other state's (`widgets::mark(colour,
  size)`: a check's result 6, a board column 7–10, "connected" 8), the square
  icons (12–15) and the loader.
- **Round is never a state**: notification dots (unread green, Doctor's
  worst finding — amber for a warning, red for a problem, else `accent` blue
  while a newer Leon is out (DOCTOR-3), none when all works — a folded project with something for the user), avatars, number
  badges.
- GitHub's own shapes stay: the issue and pull request icons, in GitHub's
  state colours (open green, draft grey, merged purple, closed red).
- Usage bars: accent, amber from 75 %, red from 90 %.

## Components

Use these; do not restyle them in place.

| Component | Function | Look |
|---|---|---|
| Button | `widgets::button` | 12 text, `surface`, `border_strong`, radius 6 |
| Primary button | `widgets::primary` | `accent` filled, `on_accent` text |
| Drag to reorder | `on_drag` / `on_drop`, `widgets::moved` | Projects: the project's line, folded, lifted (accent border, `surface`, shadow, closed hand), follows the mouse up and down only, in the pane's column, tilting with its speed about where the hand holds it (at most 4°, the far end lagging: `/` going down, `\` going up), rocking back level through the other way when the mouse stops (`drag_line.rs`: GPUI turns no box holding text, so the line is drawn in 2 px upright strips, each moved up or down); its place in the list stays empty from the drag's start and moves as it passes the others: it goes before the one under the mouse on that one's upper half, after it on its lower half (never by where that one shows, so the order holds still as the list makes room); a drop anywhere in the pane keeps that order. To do page groups: the same, by their header, the line as wide as the group (its project's mark, title, open count). CLI order: a lifted copy follows (accent border, `surface`, shadow, closed hand), the box under it is tinted `accent_soft`, a drop gives it that place |
| Button under way | `Workspace::act` + `snapshot.acting` | the same button, its label in "…ing" ("Posting…"), at 60 % opacity, a second click ignored, until the action's notice |
| Drawer header button | `header_button` (gh_detail) | icon + label, quiet until hovered. A PR's header, right to left: Open on GitHub, Copy link, Files changed, always there from the first frame; then its actions (draft, merge…), which come with its details, on their left, so nothing moves under the mouse when they appear |
| Small text action | — | 11.5, muted, hover `surface_2` ("+ Add project") |
| Segmented choice | `widgets::segmented` | options side by side, chosen one `accent_soft` |
| Section label | `widgets::label` | 10.5 uppercase faint |
| To-do row | `todos_view` | its box 16 (open: muted square; done or ticked: green checked), "<when> → <what>" side by side: an amber hourglass 13 (`warn`), the when `fg_muted` as wide as it reads up to 40 %, a faint arrow on its first line, the what as markdown with line breaks kept taking the rest, each wrapping in its own column; "Added by" 11.5 faint under it, the agent's name in its agent colour, on hover a small red bin (`trash-2` 13, `err`) at its end that removes it; done: plain, struck through, muted. Ticked or binned, it fades to 20 % over 3 s before it goes; a click on its box, or "Undo" (accent, in the bin's place), meanwhile keeps it. A double click on its when or what turns that part into an in-place edit (below). Info: under the PRs and issues, "To do" (a fold toggle: chevron 15 then the label; folded, the open count faint after it, the whole section hidden, the same in every flow), then a row like a to-do's: a faint empty box, "When…" (40 %) → "What to do", Enter in either adds it, then "N done" folded |
| To-dos button | `todos_view::todos_corner` | at the main pane's top right (headers keep 72 free): list icon 15 + how many are open, quiet until hovered, `accent_soft` on the To do page it opens. On a flow with the right pane closed, the button opening that pane (`panel-right` 16) sits right of it, the flow's header keeping 36 more free |
| In-place edit | `widgets::in_place` | a flow's name in its row, a to-do's part: the text box with no border and no focus ring, on `surface_2`, radius 4, its text at 13 exactly where the text was (its 4 inset taken back), growing with its lines |
| To do page | `todos_view::render_todos` | a page like the GitHub lists: `pane_header`, then one foldable group per flow, in the user's order, dragged by its header (`Workspace::group_with`: its project's colour (`theme::name_color` of its repository, as in the GitHub lists), title 13.5 semibold and project faint, then "Open flow" (the row link), open count), its to-do rows, its done ones folded |
| State mark | `widgets::mark`, `widgets::status_dot` | see Status |
| Fold header | `widgets::fold_toggle` | chevron 15 + title; folded state saved |
| Empty state | `widgets::empty` | 12.5 faint |
| Error in place of a list | `widgets::problem` | like an empty state, in `err` red: what failed is never taken for "nothing here" (e.g. GitHub could not be read, on the list's page; never in the left pane, where Doctor's dot says it) |
| Pane header | `widgets::pane_header` | see Layout |
| Page column | `widgets::centered` | see Layout |
| Drawer | `drawer::drawer` | see Layout; each drawer has its own id |
| Card | Doctor cards only: `rounded_xl`, `panel`, header row with icon + 14 semibold title. Lists (Info, the GitHub lists, the left pane) are plain rows and sections, never boxed | |
| PR and issue row | `gh_rows::pr_row`, `Workspace::issue_row` | the one look of a PR or an issue in every list (GitHub lists, Info, hover list): icon 16 in its state's colour (an issue's board column's), title medium, `#number` faint at the line's end (no state word, no age); under it a 12 muted line: repository, branch → base, lines, the CI indicator (one, in its state's colour: ✓ "Checks passing", □ "3/13 done", ✗ and the failing jobs' names, each opening its run, ⧄ "Checks skipped"), conflicts, labels. Folded (a merged or closed PR in Info): that first line alone, muted. In the GitHub lists, an issue's details line ends with "Open flow" (the row link) when a flow was started for it, and "Start flow" (a button) floats over the row's right end on hover, always; its drawer's header has both. No card, no colour bar |
| Pill | `attention_pill`, labels | radius full, colour at 18 %, 10.5 semibold |
| Chip | CLI, model, counts | `surface_2`, radius 4, 10.5–11 muted |
| Message box | `composer::message_box` | `surface`, `border_strong`, radius 8: the text, then a row of inline pickers and the action (Send, Stop, Start) kept right; the same in a conversation and a start page |
| Picker | `picker_look(Look::Inline / Field / Title / Branch)` | inline: 12 muted text + caret, in a message box or under it, opens upwards; field: boxed, full width, opens downwards; title: 13 semibold + faint caret, opens downwards; branch: 11.5 mono muted + caret, beside a title. A choice's second line (after `\n`) is faint mono. The list is at least 160 wide, but a title or branch picker's list hugs its choices, no wider than they are |
| File row | `file_rows` (diff_view) | status letter, mono path, folded / comments tags, lines |
| Link in a row | e.g. a check's "Failing ↗" in the PR view, a failed job's name in a row | only the text is clickable, brighter on hover |
| Notice in a conversation | `transcript` (`Entry::Notice`) | "You answered: …", "Stopped by you", a setting changed: 12 faint, left, where a tool row's name starts (20 in, past its chevron and gap), so an answer reads under the tool it allowed; never centred |
| Update card | `update_view::update_card` | Doctor's first card while there is something to say about Leon's own update (none when up to date): a Doctor card, `info` icon 16 in `accent`, the title says the state ("Leon 0.5.0 is available", "Updating to…", "…is installed", "…could not be installed"); one row: what it means, muted (the error in `err` red), then its buttons on the right — Update (primary, macOS app only) and Release notes; "Updating…" at 60 %; Restart Leon (primary); Try again and Release notes |
| Question card | composer | amber edge, "QUESTION" label, title, detail (mono 12.5 on `surface_2`, wrapping, one line each: `$ <command>`, then each path), one button per option (first primary) |
| Hover list | e.g. a flow's PRs (`flow_prs`) | `overlay`, `border_strong`, shadow, 340 wide, body text 13 set on it (it is drawn outside the window's root); PR rows folded to one line |
| A flow's PRs in its row | `flow_prs::flow_prs` | the leading state's PR icon 13 and how many, in that state's colour; in `warn` amber (the running CI indicator's) while an open one's checks run; in `err` red when an open one is blocked (a failed check, a conflict), red first; no separate mark |
| Menu | right-click (`row_menu`) | `surface`, `border_strong`, at the click |
| Flash | `flash.rs` | an action's result, top right under the header, 4 s, once |
| Copy button | `clipboard::copy_button` | copy icon 13 muted, hover `surface_2` (a code block, the right pane's branch) |
| Copied | `clipboard::copy` | "✓ Copied" just above the click, 1.2 s |
| Tooltip | — | none yet; prefer visible text |

## Icons

- Lucide, stroke 2 on 24, drawn with `icons::icon(Icon, colour, size)`;
  sizes 11–16 (13 in buttons, 14 in rows, 15–16 in headers).
- A new icon is a Lucide SVG in `assets/icons/`, added to `Icon` and its
  `ALL` list. Colour comes from a token, never from the file.
- A project is a folder: `folder-open` while its flows show, `folder` when
  folded; hovered, the fold arrow (chevron) takes its place. A repository or
  worktree (the right pane's repository line, an agent box's place) is a
  `folder` too; the branch icon is only for branches (a PR's "branch →
  base"). Leon's own logo, the bicorne of its app icon (`assets/leon.svg`) alone
  (`assets/leon-hat.png`), only on start pages.

## Motion

Only what shows work going on moves:

- **Loader**: a square that drops, squashes and turns a quarter turn over
  its shadow, every 0.5 s (`spinner.rs`), in the working blue.
- **Dragged project**: its line leans with the mouse's speed and swings
  back once into place when the mouse stops.
- **Ticked or binned to-do**: it fades over 3 s, time to undo a misclick,
  before it goes.
- **Flow box**: a working agent's colour goes round its box's border.
  With reduced motion, the loader shows instead.
- Nothing else animates: no fades, no slides.

## Words

- The glossary's words only (AGENTS.md): flow, conversation, agent, turn…
- Sentence case. Buttons say what they do ("Archive", "Copy link").
- Short units without a space: "2m", "in 3d", "5h"; a share with one:
  "34 %", "125 %". No time since ("15h ago", "checked 5m ago"): only how
  long something running has run, and when a limit resets.
- Numbers of lines as GitHub: "+52 −12", green and red.
