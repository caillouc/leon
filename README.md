# Leon

One native window to run and follow many coding-agent conversations across
many repositories, on top of the coding CLIs you already use: Claude Code,
Codex and Copilot CLI.

![A flow in Leon: the Flow tab with a planner, two coders and two reviewers spread around it, the prompts and reports between them, and the right pane's Info: the flow's pull requests, what each agent does now, its folders and issues (sample data)](docs/flow.png)

## Why Leon

Coding agents now work in parallel: a planner hands work to a coder, a
reviewer checks it, and you juggle several of these at once, in several
repositories, in several terminal tabs. Leon puts all of them in one fast
window:

- **The Flow tab** — Leon's signature. Each flow is drawn as a live diagram:
  you, every agent, and the prompts, reports and questions flowing
  between them, laid out live by the forces of CS Academy's graph editor:
  drag a box and the others make room, the arrows curving round it. Click a box to talk to that agent,
  an arrow to read what was said; the one with an answer for you is marked
  "To review".
- **What each agent does now**, live at the bottom of its conversation:
  the command it runs and for how long, its subagents, its plan, and the
  agents it waits for ("Waiting for coder 1 · Working").
- **Agents that start agents.** Any agent can start or message another
  through Leon's own tools, whichever CLI it runs on: a Claude Code planner
  can hand work to a Codex coder. Agents call each other by the names you
  see (coder 1, reviewer 2), link the issues and pull requests they create,
  and each can work in its own worktree or repository.
- **Your work around it**: the git changes and branch diff of each flow with
  line comments sent back as one review, its pull requests with their
  checks (a failing job one click away) and merge conflicts, your GitHub issues by board column, priority and sprint (start a
  flow from an issue in one click), and the connectors each CLI can reach.
- **Your subscriptions, your CLIs.** Leon drives the official CLIs with your
  own logins; it never calls a model API itself and stores everything locally.
- **Native and fast**: Rust and GPUI (Zed's UI framework), no web view.

Use it if you run more than one coding agent at a time and want to see, in
one place, who is doing what, what changed, and what waits for you.

How it works, in plain language: [docs/how-leon-works.md](docs/how-leon-works.md).
What it does: [spec/features.md](spec/features.md).

**Status: v1, under review.** Claude Code and Codex are checked end to end;
Copilot CLI is wired up but not yet tested with the real CLI.

## Install

Leon needs macOS or Linux, [rustup](https://rustup.rs), and the CLIs you want
to use, installed and logged in (`claude`, `codex`, `copilot`), plus `gh` for
GitHub. The Doctor (bottom left in the app) checks all of this and says what
to fix.

## Build and run

The toolchain version is pinned in `rust-toolchain.toml` and installed on
first build.

As a Mac app (Leon.app with its icon, in /Applications):

```bash
cargo xtask bundle --install
```

Without `--install` it only builds `target/release/Leon.app`.

macOS asks Leon for access to folders such as Documents and Downloads. With
the default ad-hoc signature it asks again after every rebuild. To keep the
answer, sign with a certificate; the bundle step picks the first it finds in
your keychain:

1. **An Apple developer account** (preferred): Xcode → Settings → Accounts →
   your account → Manage Certificates… → + → *Developer ID Application* (or
   *Apple Development*).
2. **Without one**: Keychain Access → Keychain Access menu → Certificate
   Assistant → Create a Certificate…, name **Leon Local**, identity type
   *Self Signed Root*, certificate type *Code Signing*.

`LEON_SIGN_IDENTITY` names another one.

While working on Leon itself:

```bash
cargo run
```

Only one Leon runs at a time: quit the other one first.

Linux also needs the windowing and font libraries listed in
`.github/workflows/ci.yml`.

## Checks

```bash
cargo xtask ci
```

Runs formatting, clippy (warnings are errors), tests, the crate layer check and
`cargo-deny` (`cargo install --locked cargo-deny`).

## Where things live

| Path | What |
|---|---|
| `docs/how-leon-works.md` | How Leon works, in plain language, and the choices to review. |
| `docs/agent-files.md` | How Leon reads agent and skill files (`~/.agents/`) and hands them to each CLI. |
| `spec/` | Features, decisions, architecture. Read before changing anything. |
| `AGENTS.md` | Rules for anyone (human or AI) writing code here. |
| `crates/` | The app, split by layer (see `spec/architecture.md`). |
| `docs/ui-recipes.md` | GPUI patterns to copy. |
| `xtask/` | `cargo xtask ci` and the layer check. |

## Files Leon uses

- Settings: `~/.config/leon/config.toml` (or `$XDG_CONFIG_HOME/leon/`):

  ```toml
  version = 1

  [defaults]            # prefilled for a new flow
  backend = "claude"    # claude, codex, copilot
  permission = "auto"

  [diff]
  fold = ["glob:**/*.lock", "re:^generated/"]   # files folded in diffs

  [sources]
  folders = ["~/my-agent-kit"]   # agents/, skills/, commands/, .mcp.json
  ```

  Settings (bottom left in the app) edits this file for you and keeps your
  comments.

- Agents, skills and commands are also read from the usual folders:
  `~/.agents/`, `~/.claude/`, and the project's `.agents/`, `.claude/`,
  `.github/`.
- App data: `~/Library/Application Support/leon/` on macOS,
  `~/.local/share/leon/` on Linux: the flows (`state.db`) and the
  socket agents use to reach Leon's tools (`tools.sock`).

## Licence

[GNU AGPL-3.0](LICENSE): free to use, study, change and share. Anyone who
distributes a changed Leon, or offers it as an online service, must publish
their changes under the same licence.
