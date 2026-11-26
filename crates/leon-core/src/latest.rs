//! The model picker shows each family's newest version (CONV-4): "Fable
//! 5.2", not also "Fable 5.1" and "Fable 5". A family is a model's name
//! without its version ("Claude Sonnet 5.5" → "claude sonnet"); a name
//! without a version ("Auto") is a family of its own.

use crate::ports::ModelOption;

/// The newest model of each family, in the CLI's order, and `current` too
/// when it is an older one, so the model in use stays a choice.
pub fn latest_models<'a>(models: &'a [ModelOption], current: Option<&str>) -> Vec<&'a ModelOption> {
    models
        .iter()
        .filter(|m| {
            let (family, version) = family_and_version(&m.label);
            let newer = models.iter().any(|other| {
                let (f, v) = family_and_version(&other.label);
                f == family && v > version
            });
            !newer || Some(m.id.as_str()) == current
        })
        .collect()
}

/// A model name's family and version: its words, lowercased, without the
/// ones that are numbers; those numbers, part by part ("5.5" → [5, 5]).
fn family_and_version(label: &str) -> (String, Vec<u32>) {
    let mut family = Vec::new();
    let mut version = Vec::new();
    for word in label.split([' ', '-']).filter(|w| !w.is_empty()) {
        let parts: Option<Vec<u32>> = word.split('.').map(|p| p.parse().ok()).collect();
        match parts {
            Some(parts) => version.extend(parts),
            None => family.push(word.to_lowercase()),
        }
    }
    (family.join(" "), version)
}

#[cfg(test)]
mod tests {
    use super::latest_models;
    use crate::ports::ModelOption;

    fn models(labels: &[&str]) -> Vec<ModelOption> {
        labels
            .iter()
            .map(|l| ModelOption {
                id: l.to_lowercase().replace(' ', "-"),
                label: (*l).to_owned(),
                aliases: Vec::new(),
                efforts: Vec::new(),
            })
            .collect()
    }

    fn shown(labels: &[&str], current: Option<&str>) -> Vec<String> {
        let all = models(labels);
        latest_models(&all, current)
            .into_iter()
            .map(|m| m.label.clone())
            .collect()
    }

    #[test]
    fn each_family_shows_its_newest_version() {
        let claude = [
            "Opus 5.5",
            "Fable 5.2",
            "Sonnet 5.5",
            "Haiku 5.5",
            "Fable 5.1",
            "Haiku 4.5",
            "Sonnet 5",
            "Opus 5",
            "Fable 5",
            "Opus 4.8",
            "Opus 4.10",
        ];
        assert_eq!(
            shown(&claude, None),
            ["Opus 5.5", "Fable 5.2", "Sonnet 5.5", "Haiku 5.5"]
        );
        // The model in use stays, even an older one.
        assert_eq!(
            shown(&claude, Some("fable-5.1")),
            [
                "Opus 5.5",
                "Fable 5.2",
                "Sonnet 5.5",
                "Haiku 5.5",
                "Fable 5.1"
            ]
        );
        let codex = ["GPT-6-Luna", "GPT-5.6-Terra", "GPT-5.6-Luna"];
        assert_eq!(shown(&codex, None), ["GPT-6-Luna", "GPT-5.6-Terra"]);
        let copilot = [
            "Auto",
            "Claude Sonnet 5.5",
            "Claude Sonnet 5",
            "GPT-6.2 Sol",
            "GPT-6.1 Sol",
            "MAI-Code-1.1-Flash",
            "MAI Code 2 Flash",
            "Gemini 4 Carbon",
            "Gemini 4 Argon",
            "Grok 4.7",
            "Grok 4.6",
        ];
        assert_eq!(
            shown(&copilot, None),
            [
                "Auto",
                "Claude Sonnet 5.5",
                "GPT-6.2 Sol",
                "MAI Code 2 Flash",
                "Gemini 4 Carbon",
                "Gemini 4 Argon",
                "Grok 4.7"
            ]
        );
    }
}
