//! The flash message: an action's result ("#12 is a draft again"), shown
//! once for a few seconds at the window's top right, under the header.

use std::time::Duration;

use gpui::{AnyElement, Context, IntoElement, div, prelude::*, px};

use crate::theme;
use crate::widgets::HEADER_HEIGHT;
use crate::workspace::Workspace;

/// How long a message stays.
const SHOWN: Duration = Duration::from_secs(4);

/// The flash message on screen, and the latest one already shown.
#[derive(Debug, Default)]
pub(crate) struct Flash {
    /// Id of the latest message shown: it, and older ones, never show again.
    seen: u64,
    text: Option<String>,
}

/// The core's latest message, if it has not been shown yet.
fn unseen(latest: Option<&(u64, String)>, seen: u64) -> Option<(u64, String)> {
    latest.filter(|(id, _)| *id > seen).cloned()
}

impl Workspace {
    /// Shows the core's latest flash message for a few seconds, once.
    pub(crate) fn show_flash(&mut self, cx: &mut Context<Self>) {
        let Some((id, text)) = unseen(self.snapshot.flash.as_ref(), self.flash.seen) else {
            return;
        };
        self.flash = Flash {
            seen: id,
            text: Some(text),
        };
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(SHOWN).await;
            this.update(cx, |this, cx| {
                if this.flash.seen == id {
                    this.flash.text = None;
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    pub(crate) fn render_flash(&self) -> Option<AnyElement> {
        let text = self.flash.text.clone()?;
        Some(
            div()
                .absolute()
                .top(px(HEADER_HEIGHT + 12.))
                .right_4()
                .max_w(px(360.))
                .px_3()
                .py_2()
                .rounded_lg()
                .border_1()
                .border_color(theme::border_strong())
                .bg(theme::surface())
                .shadow_lg()
                .text_size(px(12.5))
                .child(text)
                .into_any_element(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::unseen;

    #[test]
    fn a_message_shows_once() {
        let latest = (3, "#12 is a draft again".to_owned());
        let cases = [
            (None, 0, None),
            (Some(&latest), 0, Some(3)),
            (Some(&latest), 2, Some(3)),
            // Shown and gone: later snapshots carry it still, it stays gone.
            (Some(&latest), 3, None),
            (Some(&latest), 4, None),
        ];
        for (latest, seen, shown) in cases {
            assert_eq!(unseen(latest, seen).map(|(id, _)| id), shown, "seen {seen}");
        }
    }
}
