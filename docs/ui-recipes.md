# UI recipes

GPUI changes often and coding agents know it poorly (P1). Copy these patterns
from Leon's own code instead of guessing the API. Pinned versions:
`gpui-pre =0.3.12`, `gpui-pre-platform =0.3.12`, `gpui-component =0.10.0`
(workspace `Cargo.toml`). Upgrading them is a change of its own.

| Pattern | Where to copy it from |
|---|---|
| Start the app and open the window | `crates/leon-ui/src/lib.rs` → `run` |
| A view: struct + `impl Render` | `crates/leon-ui/src/workspace.rs` → `Workspace` |
| Follow the core's snapshots and re-render | `Workspace::new` (`cx.spawn` + `subscribe()` + `cx.notify()`) |
| Run something after the first frame | `Workspace::render` → `window.on_next_frame` |
| Colours | `crates/leon-ui/src/theme.rs` only; never a literal colour in a view |
| Empty state / section label | `workspace.rs` → `empty_state`, `section_label` |

Rules:

- A view reads the snapshot and renders. It never does I/O and never decides
  anything the core should decide.
- Long lists are virtualised (`uniform_list` or gpui-component's list).
- Every new pattern gets a line in this table in the same change.
