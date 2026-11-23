//! Leon's own update in Doctor (DOCTOR-3): a card above the others while a
//! newer Leon is out, being installed or waiting for a restart, or when
//! GitHub could not be asked.

use gpui::{AnyElement, Context, Div, FontWeight, IntoElement, div, prelude::*};
use leon_core::Intent;
use leon_domain::{Release, Update};

use crate::icons::{Icon, icon};
use crate::theme;
use crate::widgets::{button, primary};
use crate::workspace::Workspace;

impl Workspace {
    /// The update card; `None` while there is nothing to say (not checked
    /// yet, or this Leon is the latest).
    pub(crate) fn update_card(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let running = &self.snapshot.leon.version;
        let send =
            |intent: fn() -> Intent| cx.listener(move |this, _, _, _| this.core.send(intent()));
        let (title, detail, actions): (String, Div, Vec<AnyElement>) =
            match &self.snapshot.leon.update {
                Update::Unchecked | Update::UpToDate => return None,
                Update::Unknown(error) => (
                    "Updates".into(),
                    faint(format!("Leon could not check for a newer version: {error}")),
                    Vec::new(),
                ),
                Update::Available(r) => (
                    format!("Leon {} is available", r.version),
                    faint(format!("You have Leon {running}.")),
                    self.snapshot
                        .leon
                        .can_update
                        .then(|| {
                            primary("update-install", "Update", send(|| Intent::InstallUpdate))
                                .into_any_element()
                        })
                        .into_iter()
                        .chain([notes(r, cx)])
                        .collect(),
                ),
                Update::Installing(r) => (
                    format!("Updating to Leon {}…", r.version),
                    faint("Downloading it, checking it is Leon's own, then putting it in place."),
                    vec![
                        primary("update-install", "Updating…", |_, _, _| {})
                            .opacity(0.6)
                            .into_any_element(),
                    ],
                ),
                Update::Installed(r) | Update::Restarting(r) => (
                    format!("Leon {} is installed", r.version),
                    faint("Restart Leon to use it. Agents keep working through the restart."),
                    vec![
                        primary(
                            "update-restart",
                            "Restart Leon",
                            send(|| Intent::RestartLeon),
                        )
                        .into_any_element(),
                    ],
                ),
                Update::Failed(r, error) => (
                    format!("Leon {} could not be installed", r.version),
                    div().text_color(theme::err()).child(error.clone()),
                    vec![
                        button(
                            "update-install",
                            "Try again",
                            send(|| Intent::InstallUpdate),
                        )
                        .into_any_element(),
                        notes(r, cx),
                    ],
                ),
            };
        Some(
            div()
                .rounded_xl()
                .border_1()
                .border_color(theme::border())
                .bg(theme::panel())
                .overflow_hidden()
                .child(
                    div()
                        .px_4()
                        .py_3()
                        .flex()
                        .items_center()
                        .gap_2()
                        .border_b_1()
                        .border_color(theme::border())
                        .child(icon(Icon::Info, theme::accent(), 16.))
                        .child(
                            div()
                                .text_size(theme::rem(14.))
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(title),
                        ),
                )
                .child(
                    div()
                        .px_4()
                        .py(theme::rem(10.))
                        .flex()
                        .items_center()
                        .gap_3()
                        .child(detail.flex_1().min_w_0())
                        .children(actions),
                )
                .into_any_element(),
        )
    }
}

fn faint(text: impl Into<gpui::SharedString>) -> Div {
    div().text_color(theme::fg_muted()).child(text.into())
}

/// "Release notes": the release's GitHub page.
fn notes(release: &Release, cx: &mut Context<Workspace>) -> AnyElement {
    let page = release.page.clone();
    button(
        "update-notes",
        "Release notes",
        cx.listener(move |_, _, _, cx| cx.open_url(&page)),
    )
    .into_any_element()
}
