# Instructions for AI agents

Leon (working name) is an open-source native desktop app, in Rust, to run many
coding-agent conversations across many repositories, on top of the coding CLIs
(Claude Code, Codex, Copilot CLI).

**Phase: v1 built, under review** (2026-11-17). Milestones:
[spec/features.md](spec/features.md#milestones-proposal).

## Who owns what

| File | Owner | Agents |
|---|---|---|
| `AGENTS.md` | Maintainer | Follow it |
| [spec/features.md](spec/features.md) — what the app does | Maintainer | Read it; add or update a feature only when the maintainer explicitly asks for it |
| [spec/decisions.md](spec/decisions.md) — technical and product choices | Maintainer | Read it; record a decision only when the maintainer states it, in their words, never one of your own |
| [spec/architecture.md](spec/architecture.md) — how the code is organised | Agents | Keep it true to the code, within decisions.md |
| Code, tests, CI | Agents | |

## Hard rules

1. **Build only what features.md lists, the simplest way that does the job.**
   No extra feature, setting, button or option; no complex mechanism when a
   simple one works. If something seems missing, ask.
2. **Never decide what decisions.md doesn't cover**: a structural dependency, a
   file or storage format, a process, a new pattern or crate. Ask, with the
   options and your recommendation.
3. **Follow the conventions below.**
4. **Meet the speed targets** in architecture.md (Zed is the reference).
   Nothing blocks the UI thread.
5. **Never duplicate code.** Reuse or extend what exists; merge the copies you
   meet in the code you touch.
6. **Generic only.** Nothing specific to one user's agents, repos or workflow in
   code, defaults or test data.
7. **Verify, don't infer.** A fact about an external tool (CLI flag, file path,
   protocol field) comes from its docs or a real run, with the date. Otherwise
   write `⚠️ not verified`.
8. **Safety.** Tests never touch a real repository, the network or a real
   account. No secrets or personal data in code, fixtures or logs.
9. **Prefer the reliable solution** (D6): an official CLI over a hand-written
   API client, a proven mechanism over a clever one.

When blocked — unclear feature, missing decision, two readings that lead to
different code — stop and ask: the question, the options, your recommendation.

## Conventions

### Structure

- Layers and crates: [spec/architecture.md](spec/architecture.md). Dependencies
  point one way; CI checks it.
- Every outside system (CLIs, git, GitHub, file watcher, clock, storage) sits
  behind a trait owned by the core. Tests use fakes.
- The UI holds no logic: it renders state and sends intents.
- One responsibility per module. Guideline: file ≤ 400 lines, function ≤ 50.
- No dead code, no commented-out code, no `TODO` without an issue link.

### Rust

- Edition 2024, toolchain pinned in `rust-toolchain.toml`, default `rustfmt`.
- Clippy `-D warnings` with `all` + `pedantic`; outside tests, `unwrap_used`,
  `expect_used`, `panic`, `todo`, `dbg_macro`, `print_stdout`, `print_stderr`
  are denied.
- `#![forbid(unsafe_code)]` in every crate.
- Errors: `thiserror` per crate, `anyhow` only in the binary. Errors say which
  conversation or command failed. Never ignore a `Result`.
- Logging: `tracing`, structured. Prompts, messages and diffs never above
  `debug`; secrets never.
- Newtype ids, enums for states, exhaustive `match` on our own enums.
- Anything persisted or read from outside is versioned and tested on fixtures.
  Unknown fields from a CLI are ignored, never a crash.
- A new crate is justified in the PR (why, maintenance, licence, size); a
  structural one needs a decision. `cargo-deny` runs in CI.

### Tests

| What | How |
|---|---|
| Domain logic | Table-driven unit tests |
| CLI adapters | Replay real CLI output recorded in `fixtures/<cli>/<cli-version>/`, snapshot with `insta`; scrubbed of personal data |
| Real CLIs | Opt-in contract tests, not in default CI |
| git | Temporary repos created by the test |
| GitHub | Recorded HTTP responses |
| UI | View-model tests, no rendering |
| Performance | Benchmarks on hot paths, checked against the targets |

A bug fix starts with a test that fails.

### Git and PRs

- Default branch `main`. Branches: short kebab-case slug, no ticket id.
- Commits: Conventional Commits, `type(scope): summary #<issue>`.
- PR body: **Why** / **What**, feature IDs, behaviour changes. CI green on macOS
  and Linux.

## Definition of done

- Cites the feature ID(s) it implements.
- Tests added; a bug fix has a test that failed before.
- `cargo xtask ci` passes (fmt, clippy, tests, layer check, cargo-deny).
- architecture.md updated if the structure changed.
- The report lists questions for the maintainer and anything not verified.

## Glossary

One word per concept, in code, UI and docs. A new concept: ask to add it here.

| Term | Meaning |
|---|---|
| Project | A folder registered in the app, usually a git repo. Groups flows. |
| Flow | One left-pane item: the conversations the user started in it, every conversation an agent started from them, and who started which, with the prompts sent and the answers returned (its diagram). In code: `Conversation`. |
| Conversation | One agent's own CLI conversation in a flow (its id, its transcript, one working directory at a time), started by the user or by another agent. In code: `AgentSession`. |
| Turn | One user message and everything the agent does until it waits again. |
| Backend | A coding CLI the app drives (`claude`, `codex`, `copilot`). Not an agent's *tool*. |
| Agent | A named definition (markdown + frontmatter) the user talks to directly. |
| Subagent | An agent the CLI itself runs inside a conversation. Shown only as counts. |
| Pending review | Diff comments collected before being sent as one message. |
| Skill | A folder with a `SKILL.md` (Agent Skills standard). |
| Tool | What an agent calls in a turn: a CLI built-in or an MCP tool. |
| Working directory | Where a conversation's CLI runs: the main checkout or a worktree. |
| Base | Branch and commit a flow's branch started from. |
| Status | `idle`, `running`, `needs_input`, `error`. |
