//! Copying from Leon's interface (QUAL-5): a code block, the worktree's
//! branch, a pull request's link. The text goes to the clipboard and
//! "Copied" shows for a moment just above where the user clicked.

use std::time::{Duration, Instant};

use gpui::{
    Anchor, AnyElement, App, ClipboardItem, IntoElement, Pixels, Point, Window, anchored, deferred,
    div, point, prelude::*, px,
};

use crate::icons::{Icon, icon};
use crate::theme;

/// How long "Copied" stays.
const SHOWN: Duration = Duration::from_millis(1200);

/// Where, in the window, and when the last copy was made.
struct LastCopy {
    at: Point<Pixels>,
    when: Instant,
}

impl gpui::Global for LastCopy {}

/// Puts `text` on the clipboard and shows "Copied" where the user clicked.
pub(crate) fn copy(window: &Window, cx: &mut App, text: String) {
    cx.write_to_clipboard(ClipboardItem::new_string(text));
    cx.set_global(LastCopy {
        at: window.mouse_position(),
        when: Instant::now(),
    });
    cx.refresh_windows();
    cx.spawn(async move |cx| {
        cx.background_executor().timer(SHOWN).await;
        cx.update(App::refresh_windows);
    })
    .detach();
}

/// "Copied", centred just above the last copy's click, while it is recent.
pub(crate) fn render_copied(cx: &App) -> Option<AnyElement> {
    let last = cx.try_global::<LastCopy>()?;
    if last.when.elapsed() >= SHOWN {
        return None;
    }
    let pill = div()
        .flex()
        .items_center()
        .gap(theme::rem(4.))
        .px_2()
        .py(theme::rem(3.))
        .rounded_md()
        .border_1()
        .border_color(theme::border_strong())
        .bg(theme::surface_2())
        .shadow_lg()
        .text_size(theme::rem(12.))
        .text_color(theme::fg())
        .child(icon(Icon::Passed, theme::ok(), 12.))
        .child("Copied");
    Some(
        deferred(
            anchored()
                .position(point(last.at.x, last.at.y - px(10.)))
                .anchor(Anchor::BottomCenter)
                .snap_to_window_with_margin(px(8.))
                .child(pill),
        )
        .with_priority(3)
        .into_any_element(),
    )
}
