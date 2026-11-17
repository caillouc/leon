# Leon

One native window to run and follow many coding-agent conversations across
many repositories, on top of the coding CLIs you already use: Claude Code,
Codex, Copilot CLI and Antigravity.

**Status: milestone M0 (foundations).** The window opens with its three panes;
no conversation features yet. What Leon does and in which order:
[spec/features.md](spec/features.md).

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
| `spec/` | Features, decisions, architecture. Read before changing anything. |
| `AGENTS.md` | Rules for anyone (human or AI) writing code here. |
| `crates/` | The app, split by layer (see `spec/architecture.md`). |
| `docs/ui-recipes.md` | GPUI patterns to copy. |
| `xtask/` | `cargo xtask ci` and the layer check. |

## Files Leon uses

- Settings: `~/.config/leon/config.toml` (or `$XDG_CONFIG_HOME/leon/`).
- App data: `~/Library/Application Support/leon/` on macOS,
  `~/.local/share/leon/` on Linux.

## Licence

MIT or Apache-2.0, at your option.
