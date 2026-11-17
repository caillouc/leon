# Leon

One native window to run and follow many coding-agent conversations across
many repositories, on top of the coding CLIs you already use: Claude Code,
Codex and Copilot CLI.

**Status: working towards v1.** Conversations with Claude Code and Codex,
agents starting other agents (flow view), git changes and branch diff with
review comments, GitHub pull requests, and Copilot CLI (not yet tested with
the real CLI). What Leon does: [spec/features.md](spec/features.md).

Leon drives the CLIs with your own logins and subscriptions; it never calls a
model API itself. Install and log in to each CLI you want to use, and to `gh`
for GitHub.

## Build and run

Needs macOS or Linux, and [rustup](https://rustup.rs). The toolchain version is
pinned in `rust-toolchain.toml` and installed on first build.

```bash
cargo run -p leon
```

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
| `spec/` | Features, decisions, architecture. Read before changing anything. |
| `AGENTS.md` | Rules for anyone (human or AI) writing code here. |
| `crates/` | The app, split by layer (see `spec/architecture.md`). |
| `docs/ui-recipes.md` | GPUI patterns to copy. |
| `xtask/` | `cargo xtask ci` and the layer check. |

## Files Leon uses

- Settings: `~/.config/leon/config.toml` (or `$XDG_CONFIG_HOME/leon/`):

  ```toml
  version = 1

  [defaults]            # prefilled for a new conversation
  backend = "claude"    # claude, codex, copilot
  permission = "auto"

  [diff]
  fold = ["glob:**/*.lock", "re:^generated/"]   # files folded in diffs

  [sources]
  folders = ["~/my-agent-kit"]   # agents/, skills/, commands/, .mcp.json
  ```

- Agents, skills and commands are also read from the usual folders:
  `~/.agents/`, `~/.claude/`, and the project's `.agents/`, `.claude/`,
  `.github/`.
- App data: `~/Library/Application Support/leon/` on macOS,
  `~/.local/share/leon/` on Linux: the conversations (`state.db`) and the
  socket agents use to reach Leon's tools (`tools.sock`).

## Licence

MIT or Apache-2.0, at your option.
