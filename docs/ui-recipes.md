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
| Empty state, section label, buttons, status shape | `crates/leon-ui/src/widgets.rs` |
| Dropdown (one open at a time) | `crates/leon-ui/src/picker.rs` → `Workspace::picker` |
| Virtualised transcript that follows the end | `crates/leon-ui/src/transcript.rs` + `Workspace::list_for` |
| Markdown | `gpui_component::text::TextView::markdown(id, text)` in `transcript.rs` |
| Multi-line input, Enter sends | `TextareaState::new(..).submit_on_enter(true)` + `InputEvent::PressEnter` in `workspace.rs` |
| Virtualised rows of different kinds, reset when they change (diff) | `crates/leon-ui/src/diff_rows.rs` → `diff_rows` + `diff_list` |
| Show a child only while its row is hovered | `group(..)` on the row + `group_hover(..)` on the child, in `diff_view.rs` → `line_row` |
| Focus an input | `entity.focus_handle(cx).focus(window, cx)` (trait `Focusable`), in `diff_view.rs` → `start_comment` |
| A short message that disappears | `Workspace::show_flash` (timer with `cx.spawn` + `background_executor().timer`) |
| Canvas with pan, zoom, drag and painted arrows | `crates/leon-ui/src/flow.rs` (`canvas` + `PathBuilder`, mouse and `on_scroll` / `on_pinch` listeners) |
| A footer row pinned under a scrolling list | `crates/leon-ui/src/nav.rs` (`render_bottom_nav`, after the list in `left.rs`) |
| Look at the UI without a screen | `cargo run -p leon-ui --example screenshots --features preview -- <folder>` writes PNGs of the main screens |

Rules:

- GPUI allows one `hover` style per element: set it once (see `widgets::base_button`).
- A view reads the snapshot and renders. It never does I/O and never decides
  anything the core should decide.
- Long lists are virtualised (`uniform_list` or gpui-component's list).
- Every new pattern gets a line in this table in the same change.
| A panel over the window that blocks the mouse behind it | `crates/leon-ui/src/drawer.rs` (`.occlude()` on the backdrop) |
| An icon | `crates/leon-ui/src/icons.rs` → `icon(Icon::…, colour, size)`; add the Lucide SVG to `assets/icons/` |
| A shortcut | `crates/leon-ui/src/shortcuts.rs` → `bindings()`: write `cmd-`/`ctrl-` via `primary!`, never `secondary-` (it panics) |

