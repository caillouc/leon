//! The models each CLI offers (I31, CONV-4), each with its effort levels
//! when the CLI lists them, the CLI's default first. Claude Code and Codex
//! are asked at launch, without a turn: an `initialize` control request on
//! stream-json (2.1.332), app-server `model/list` (0.160.0). Copilot CLI
//! lists them in each session it opens over ACP (1.0.114): read from its
//! conversations' sessions, as asking would leave an empty session in its
//! history. Real runs, 2026-11-26.

use std::ffi::OsString;

use leon_core::ports::ModelOption;
use leon_domain::BackendKind;
use serde_json::{Value, json};

use crate::ask::ask;

/// The models `backend` offers now; `None` when it could not tell, or is
/// not asked (Copilot CLI: from its sessions, [`acp_models`]).
pub(crate) fn read(path: Option<&OsString>, backend: BackendKind) -> Option<Vec<ModelOption>> {
    match backend {
        BackendKind::ClaudeCode => claude(path),
        BackendKind::Codex => codex(path),
        BackendKind::CopilotCli => None,
    }
}

/// Codex app-server's `initialize`, answered as id 1.
pub(crate) fn codex_initialize() -> Value {
    json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize",
        "params": { "clientInfo": { "name": "leon", "version": env!("CARGO_PKG_VERSION") } } })
}

fn claude(path: Option<&OsString>) -> Option<Vec<ModelOption>> {
    crate::ask::claude_control(path, "initialize").map(|answer| claude_models(&answer))
}

fn codex(path: Option<&OsString>) -> Option<Vec<ModelOption>> {
    ask(
        path,
        "codex",
        &["app-server"],
        &codex_initialize(),
        |v, send| match v["id"].as_i64() {
            Some(1) => {
                let sent = send(json!({ "jsonrpc": "2.0", "method": "initialized" }))
                    && send(
                        json!({ "jsonrpc": "2.0", "id": 2, "method": "model/list", "params": {} }),
                    );
                (!sent).then(Vec::new)
            }
            Some(2) => Some(codex_models(&v["result"])),
            _ => None,
        },
    )
}

/// Claude Code's `initialize` answer: its `models`, each by the full id
/// it runs (`resolvedModel`), its aliases (`opus`…) folded in; the
/// `default` entry puts its model first. A family's first model also takes
/// the family's name (`fable`), as `--help` names them "an alias for the
/// latest model".
pub(crate) fn claude_models(answer: &Value) -> Vec<ModelOption> {
    let entries = answer["models"].as_array().map_or(&[][..], Vec::as_slice);
    let mut models: Vec<ModelOption> = Vec::new();
    let mut default = None;
    for entry in entries {
        let (Some(value), Some(id)) = (entry["value"].as_str(), entry["resolvedModel"].as_str())
        else {
            continue;
        };
        if value == "default" {
            default = Some(id.to_owned());
            continue;
        }
        let ix = models.iter().position(|m| m.id == id).unwrap_or_else(|| {
            models.push(ModelOption {
                id: id.to_owned(),
                label: entry["displayName"].as_str().unwrap_or(id).to_owned(),
                aliases: Vec::new(),
                efforts: strings(&entry["supportedEffortLevels"]),
            });
            models.len() - 1
        });
        let model = &mut models[ix];
        if value != id && !model.aliases.iter().any(|a| a == value) {
            model.aliases.push(value.to_owned());
        }
    }
    family_aliases(&mut models);
    first(models, default.as_deref())
}

/// Codex's `model/list` answer: its models not hidden, `isDefault` first.
pub(crate) fn codex_models(result: &Value) -> Vec<ModelOption> {
    let data = result["data"].as_array().map_or(&[][..], Vec::as_slice);
    let default = data
        .iter()
        .find(|m| m["isDefault"] == true)
        .and_then(|m| m["id"].as_str());
    let models = data
        .iter()
        .filter(|m| m["hidden"] != true)
        .filter_map(|m| {
            let id = m["id"].as_str()?;
            Some(ModelOption {
                id: id.to_owned(),
                label: m["displayName"].as_str().unwrap_or(id).to_owned(),
                aliases: Vec::new(),
                efforts: m["supportedReasoningEfforts"]
                    .as_array()
                    .map_or(&[][..], Vec::as_slice)
                    .iter()
                    .filter_map(|e| e["reasoningEffort"].as_str().map(str::to_owned))
                    .collect(),
            })
        })
        .collect();
    first(models, default)
}

/// An ACP session's answer (Copilot CLI's `session/new`):
/// `models.availableModels`, the current one first. Efforts are not listed
/// per model.
pub(crate) fn acp_models(result: &Value) -> Vec<ModelOption> {
    let models = &result["models"];
    let list = models["availableModels"]
        .as_array()
        .map_or(&[][..], Vec::as_slice)
        .iter()
        .filter_map(|m| {
            let id = m["modelId"].as_str()?;
            Some(ModelOption {
                id: id.to_owned(),
                label: m["name"].as_str().unwrap_or(id).to_owned(),
                aliases: Vec::new(),
                efforts: Vec::new(),
            })
        })
        .collect();
    first(list, models["currentModelId"].as_str())
}

/// `models` with `default` moved first.
fn first(mut models: Vec<ModelOption>, default: Option<&str>) -> Vec<ModelOption> {
    if let Some(ix) = default.and_then(|d| models.iter().position(|m| m.id == d)) {
        let model = models.remove(ix);
        models.insert(0, model);
    }
    models
}

/// Each family's first model takes its family's name as an alias, unless
/// a model already has it: "Fable 5.2" → `fable`.
fn family_aliases(models: &mut [ModelOption]) {
    for ix in 0..models.len() {
        let Some(family) = models[ix]
            .label
            .split_whitespace()
            .next()
            .map(str::to_lowercase)
        else {
            continue;
        };
        let taken = models
            .iter()
            .any(|m| m.id == family || m.aliases.contains(&family));
        if !taken {
            models[ix].aliases.push(family);
        }
    }
}

fn strings(value: &Value) -> Vec<String> {
    value
        .as_array()
        .map_or(&[][..], Vec::as_slice)
        .iter()
        .filter_map(|v| v.as_str().map(str::to_owned))
        .collect()
}

#[cfg(test)]
mod tests {
    use serde_json::Value;

    use super::{acp_models, claude_models, codex_models};

    fn line(text: &str) -> Value {
        serde_json::from_str(text.lines().next().unwrap()).unwrap()
    }

    fn summary(models: &[leon_core::ports::ModelOption]) -> Vec<(String, String, Vec<String>)> {
        models
            .iter()
            .map(|m| (m.id.clone(), m.label.clone(), m.aliases.clone()))
            .collect()
    }

    /// Opt-in: asks the CLIs installed here, with no turn
    /// (`cargo test -p leon-backends -- --ignored real_clis`).
    #[test]
    #[ignore = "runs the real CLIs"]
    fn real_clis_list_their_models() {
        use leon_domain::BackendKind;
        for backend in [BackendKind::ClaudeCode, BackendKind::Codex] {
            let models = super::read(None, backend).unwrap_or_default();
            let ids: Vec<&str> = models.iter().take(4).map(|m| m.id.as_str()).collect();
            println!("{backend:?}: {} models, first {ids:?}", models.len());
            assert!(!models.is_empty(), "{backend:?}");
        }
    }

    #[test]
    fn claude_code_lists_its_models_by_full_id() {
        let answer = line(include_str!("../fixtures/claude/2.1.332/initialize.jsonl"));
        let models = claude_models(&answer["response"]["response"]);
        let s = |id: &str, label: &str, aliases: &[&str]| {
            (
                id.to_owned(),
                label.to_owned(),
                aliases.iter().map(|a| (*a).to_owned()).collect::<Vec<_>>(),
            )
        };
        assert_eq!(
            summary(&models)[..5],
            [
                s("claude-opus-5-5", "Opus 5.5", &["opus"]),
                s("claude-fable-5-2", "Fable 5.2", &["fable"]),
                s("claude-sonnet-5-5", "Sonnet 5.5", &["sonnet"]),
                s("claude-haiku-5-5", "Haiku 5.5", &["haiku"]),
                s("claude-fable-5-1", "Fable 5.1", &[]),
            ]
        );
        assert_eq!(models[0].efforts, ["low", "medium", "high", "xhigh", "max"]);
        // Haiku 4.5 lists no effort; Opus 4.6 none above `high` but `max`.
        let effort = |id: &str| models.iter().find(|m| m.id == id).unwrap().efforts.clone();
        assert_eq!(effort("claude-haiku-4-5-20251001"), [] as [String; 0]);
        assert_eq!(effort("claude-opus-4-6"), ["low", "medium", "high", "max"]);
    }

    #[test]
    fn codex_lists_its_models_with_their_efforts() {
        let answer = line(include_str!("../fixtures/codex/0.160.0/model-list.jsonl"));
        let models = codex_models(&answer["result"]);
        let ids: Vec<&str> = models.iter().map(|m| m.id.as_str()).collect();
        assert_eq!(ids, ["gpt-6-luna", "gpt-5.6-terra", "gpt-5.6-luna"]);
        assert_eq!(models[0].label, "GPT-6-Luna");
        assert!(models[1].efforts.iter().any(|e| e == "ultra"));
    }

    #[test]
    fn copilot_lists_its_models_the_current_first() {
        let answer = line(include_str!(
            "../fixtures/copilot/1.0.114/session-new.jsonl"
        ));
        let models = acp_models(&answer["result"]);
        assert_eq!(models[0].id, "claude-sonnet-5.5");
        assert_eq!(models[0].label, "Claude Sonnet 5.5");
        assert!(models.iter().any(|m| m.id == "auto"));
        assert_eq!(models.len(), 26);
    }
}
