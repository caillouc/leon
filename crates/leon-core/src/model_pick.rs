//! An agent file's `model` picks the CLI (EXT-6).

use std::collections::HashMap;

use leon_domain::{BackendKind, ConversationSettings, cli_for_model, full_cli_order};

use crate::ports::BackendOptions;

/// `settings` for an agent whose file names `model`: the first CLI in
/// `order` (completed with the others) that offers it, with that model. On
/// another CLI, the effort stays if that CLI has it and the permission
/// becomes its first ("auto"). `None` when the file names no model,
/// `inherit`, or a model no CLI offers.
pub fn settings_for_agent<S: std::hash::BuildHasher>(
    settings: &ConversationSettings,
    model: Option<&str>,
    order: &[BackendKind],
    options: &HashMap<BackendKind, BackendOptions, S>,
) -> Option<ConversationSettings> {
    let names = |backend| {
        options
            .get(&backend)
            .map_or_default(BackendOptions::model_names)
    };
    let (backend, model) = cli_for_model(model?, &full_cli_order(order), names)?;
    let mut next = settings.clone();
    if backend != settings.backend {
        let target = options.get(&backend);
        next.effort = settings
            .effort
            .clone()
            .filter(|e| target.is_some_and(|o| o.efforts.contains(e)));
        next.permission = target
            .and_then(|o| o.permissions.first().cloned())
            .unwrap_or_else(|| "auto".to_owned());
        next.backend = backend;
    }
    next.model = Some(model);
    Some(next)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use leon_domain::BackendKind::{ClaudeCode, Codex};
    use leon_domain::ConversationSettings;

    use super::settings_for_agent;
    use crate::ports::{BackendOptions, ModelOption};

    #[test]
    fn the_model_picks_the_cli_and_its_settings() {
        let options =
            |models: &[(&str, &[&str])], efforts: &[&str], permissions: &[&str]| BackendOptions {
                models: models
                    .iter()
                    .map(|(id, aliases)| ModelOption {
                        id: (*id).into(),
                        label: (*id).into(),
                        aliases: aliases.iter().map(|a| (*a).to_owned()).collect(),
                    })
                    .collect(),
                efforts: efforts.iter().map(|e| (*e).to_owned()).collect(),
                default_effort: None,
                permissions: permissions.iter().map(|p| (*p).to_owned()).collect(),
            };
        let all = HashMap::from([
            (
                ClaudeCode,
                options(
                    &[("claude-opus-5-5", &["opus"])],
                    &["high", "max"],
                    &["auto", "plan"],
                ),
            ),
            (
                Codex,
                options(
                    &[("gpt-6-luna", &[])],
                    &["low", "high"],
                    &["auto", "read only"],
                ),
            ),
        ]);
        let planner = ConversationSettings {
            backend: ClaudeCode,
            agent: Some("coder".into()),
            model: Some("claude-opus-5-5".into()),
            effort: Some("max".into()),
            permission: "plan".into(),
        };
        // To Codex: its model, `max` it does not have, its first permission.
        let coder = settings_for_agent(&planner, Some("gpt-6-luna"), &[], &all).unwrap();
        assert_eq!(
            (
                coder.backend,
                coder.model.as_deref(),
                coder.effort.clone(),
                coder.permission.as_str()
            ),
            (Codex, Some("gpt-6-luna"), None, "auto")
        );
        // Codex first in the order, but only Claude Code offers `opus`.
        let reviewer = settings_for_agent(&coder, Some("opus"), &[Codex], &all).unwrap();
        assert_eq!(
            (
                reviewer.backend,
                reviewer.model.as_deref(),
                reviewer.permission.as_str()
            ),
            (ClaudeCode, Some("claude-opus-5-5"), "auto")
        );
        // Same CLI: only the model changes.
        let same = settings_for_agent(&planner, Some("opus"), &[], &all).unwrap();
        assert_eq!(same, planner);
        for model in [None, Some("inherit"), Some("llama-9")] {
            assert_eq!(
                settings_for_agent(&planner, model, &[], &all),
                None,
                "{model:?}"
            );
        }
    }
}
