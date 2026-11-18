//! The CLI, model, effort and permission pickers (CONV-3, CONV-4, CONV-5),
//! shared by the message box and the new-flow form. A value left
//! unchosen shows what will be used (the core fills it in), never "default".

use gpui::{AnyElement, Context, Window};
use leon_domain::{BackendKind, ConversationSettings};

use crate::picker::{Choice, Look};
use crate::workspace::Workspace;

impl Workspace {
    /// The model's name and the effort a conversation runs with, defaults
    /// included: what the CLI is given, never "default".
    pub(crate) fn model_and_effort(&self, settings: &ConversationSettings) -> (String, String) {
        let options = self.backend_options(settings.backend);
        let model = settings
            .model
            .as_deref()
            .or_else(|| options.models.first().map(|m| m.id.as_str()))
            .map_or_else(String::new, |id| options.model_label(id));
        let effort = settings
            .effort
            .clone()
            .or_else(|| options.default_effort.clone())
            .unwrap_or_default();
        (model, effort)
    }

    /// The four pickers, in order: CLI, model, effort, permission.
    /// `on_change` gets the new settings.
    pub(crate) fn setting_pickers(
        &self,
        look: Look,
        prefix: &str,
        settings: &ConversationSettings,
        cx: &mut Context<Self>,
        on_change: impl Fn(&mut Self, ConversationSettings, &mut Window, &mut Context<Self>)
        + Clone
        + 'static,
    ) -> [AnyElement; 4] {
        let options = self.backend_options(settings.backend);
        let inline = look == Look::Inline;
        let backends: Vec<Choice> = BackendKind::ALL
            .into_iter()
            .filter(|b| !self.backend_options(*b).permissions.is_empty())
            .map(|b| (b.id().to_owned(), b.display_name().to_owned()))
            .collect();
        let backend = {
            let (s, f) = (settings.clone(), on_change.clone());
            self.picker_look(
                look,
                &format!("{prefix}-backend"),
                settings.backend.display_name(),
                backends,
                cx,
                move |this, value, w, cx| {
                    let Some(kind) = BackendKind::from_id(&value) else {
                        return;
                    };
                    let permission = this.backend_options(kind).permissions.first().cloned();
                    let next = ConversationSettings {
                        backend: kind,
                        model: None,
                        effort: None,
                        permission: permission.unwrap_or_else(|| "auto".to_owned()),
                        ..s.clone()
                    };
                    f(this, next, w, cx);
                },
            )
        };
        let model = {
            let (s, f) = (settings.clone(), on_change.clone());
            let current = self.model_and_effort(settings).0;
            let choices = options
                .models
                .iter()
                .map(|m| (m.id.clone(), m.label.clone()))
                .collect();
            self.picker_look(
                look,
                &format!("{prefix}-model"),
                &current,
                choices,
                cx,
                move |this, value, w, cx| {
                    f(
                        this,
                        ConversationSettings {
                            model: Some(value),
                            ..s.clone()
                        },
                        w,
                        cx,
                    );
                },
            )
        };
        let effort = {
            let (s, f) = (settings.clone(), on_change.clone());
            let value = self.model_and_effort(settings).1;
            let current = if inline {
                format!("effort {value}")
            } else {
                value
            };
            let choices = options
                .efforts
                .iter()
                .map(|e| (e.clone(), e.clone()))
                .collect();
            self.picker_look(
                look,
                &format!("{prefix}-effort"),
                &current,
                choices,
                cx,
                move |this, value, w, cx| {
                    f(
                        this,
                        ConversationSettings {
                            effort: Some(value),
                            ..s.clone()
                        },
                        w,
                        cx,
                    );
                },
            )
        };
        let permission = {
            let (s, f) = (settings.clone(), on_change);
            let choices = options
                .permissions
                .iter()
                .map(|p| (p.clone(), p.clone()))
                .collect();
            self.picker_look(
                look,
                &format!("{prefix}-permission"),
                &settings.permission,
                choices,
                cx,
                move |this, value, w, cx| {
                    f(
                        this,
                        ConversationSettings {
                            permission: value,
                            ..s.clone()
                        },
                        w,
                        cx,
                    );
                },
            )
        };
        [backend, model, effort, permission]
    }
}
