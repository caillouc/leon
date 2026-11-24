use std::fmt;
use std::path::Path;

use leon_domain::{BackendKind, ConversationDefaults, Editor, OpenOn, Settings, Zoom};
use toml::{Table, Value};

/// Config format version this build reads.
pub const CURRENT_VERSION: i64 = 1;

/// Settings read from a file, with the problems found on the way.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Loaded {
    /// The settings, defaults filled in.
    pub settings: Settings,
    /// One entry per ignored or invalid value.
    pub warnings: Vec<Warning>,
}

/// A config problem: which key, and what is wrong with it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Warning {
    /// Dotted key, e.g. `defaults.backend`. Empty for the whole file.
    pub key: String,
    /// What is wrong, and what Leon does instead.
    pub message: String,
}

impl Warning {
    fn new(key: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            message: message.into(),
        }
    }
}

impl fmt::Display for Warning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.key.is_empty() {
            f.write_str(&self.message)
        } else {
            write!(f, "{}: {}", self.key, self.message)
        }
    }
}

/// Reads the file at `path`. A missing file gives the defaults, silently.
pub fn load(path: &Path) -> Loaded {
    match std::fs::read_to_string(path) {
        Ok(text) => parse(&text),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Loaded::default(),
        Err(error) => Loaded {
            settings: Settings::default(),
            warnings: vec![Warning::new(
                "",
                format!("cannot read {}: {error}; using defaults", path.display()),
            )],
        },
    }
}

/// Parses config text.
pub fn parse(text: &str) -> Loaded {
    let mut out = Loaded::default();
    let table: Table = match text.parse() {
        Ok(table) => table,
        Err(error) => {
            out.warnings.push(Warning::new(
                "",
                format!("not valid TOML ({}); using defaults", error.message()),
            ));
            return out;
        }
    };
    for (key, value) in &table {
        match key.as_str() {
            "version" => check_version(value, &mut out.warnings),
            "defaults" => read_defaults(value, &mut out.settings.defaults, &mut out.warnings),
            "diff" => {
                out.settings.diff_fold = read_list(value, "diff", "fold", &mut out.warnings, |p| {
                    leon_domain::FoldPattern::parse(p)
                        .map(drop)
                        .ok_or_else(|| format!("\"{p}\" is not a valid glob or regex, ignored"))
                });
            }
            "agents" => {
                out.settings.cli_order =
                    read_list(value, "agents", "cli_order", &mut out.warnings, |id| {
                        BackendKind::from_id(id)
                            .map(drop)
                            .ok_or_else(|| format!("unknown CLI \"{id}\", ignored"))
                    })
                    .iter()
                    .filter_map(|id| BackendKind::from_id(id))
                    .collect();
            }
            "ui" => read_ui(value, &mut out.settings, &mut out.warnings),
            "sources" => {
                out.settings.sources =
                    read_list(value, "sources", "folders", &mut out.warnings, |f| {
                        if f.trim().is_empty() {
                            Err("empty folder, ignored".to_owned())
                        } else {
                            Ok(())
                        }
                    });
            }
            other => out
                .warnings
                .push(Warning::new(other, "unknown key, ignored")),
        }
    }
    out
}

fn check_version(value: &Value, warnings: &mut Vec<Warning>) {
    match value.as_integer() {
        Some(CURRENT_VERSION) => {}
        Some(other) => warnings.push(Warning::new(
            "version",
            format!("this Leon reads version {CURRENT_VERSION}, the file says {other}; reading it anyway"),
        )),
        None => warnings.push(Warning::new("version", "must be a number")),
    }
}

/// `[ui]`: the text size and what a flow opens on (SETTINGS-2, SETTINGS-3).
fn read_ui(value: &Value, settings: &mut Settings, warnings: &mut Vec<Warning>) {
    let Some(table) = value.as_table() else {
        warnings.push(Warning::new("ui", "must be a table, ignored"));
        return;
    };
    for (key, value) in table {
        match key.as_str() {
            "zoom" => match value
                .as_integer()
                .and_then(|n| u16::try_from(n).ok())
                .filter(|n| Zoom::RANGE.contains(n))
            {
                Some(size) => settings.zoom = Zoom(size),
                None => warnings.push(Warning::new(
                    "ui.zoom",
                    format!(
                        "must be a percentage from {} to {}, ignored",
                        Zoom::RANGE.start(),
                        Zoom::RANGE.end()
                    ),
                )),
            },
            "open_flows_on" => match value.as_str().and_then(OpenOn::from_id) {
                Some(open) => settings.open_on = open,
                None => warnings.push(Warning::new(
                    "ui.open_flows_on",
                    "must be \"flow\" or \"conversation\", ignored",
                )),
            },
            "editor" => match value.as_str().and_then(Editor::from_id) {
                Some(editor) => settings.editor = Some(editor),
                None => warnings.push(Warning::new(
                    "ui.editor",
                    "must be \"vscode\" or \"zed\", ignored",
                )),
            },
            other => warnings.push(Warning::new(format!("ui.{other}"), "unknown key, ignored")),
        }
    }
}

fn read_defaults(value: &Value, defaults: &mut ConversationDefaults, warnings: &mut Vec<Warning>) {
    let Some(table) = value.as_table() else {
        warnings.push(Warning::new("defaults", "must be a table, ignored"));
        return;
    };
    for (key, value) in table {
        let dotted = format!("defaults.{key}");
        let Some(text) = value.as_str() else {
            warnings.push(Warning::new(dotted, "must be a string, ignored"));
            continue;
        };
        match key.as_str() {
            "backend" => match BackendKind::from_id(text) {
                Some(kind) => defaults.backend = kind,
                None => warnings.push(Warning::new(
                    dotted,
                    format!(
                        "unknown backend \"{text}\", using \"{}\"",
                        defaults.backend.id()
                    ),
                )),
            },
            "agent" => defaults.agent = Some(text.to_owned()).filter(|a| !a.is_empty()),
            "model" => defaults.model = Some(text.to_owned()),
            "effort" => defaults.effort = Some(text.to_owned()),
            "permission" => text.clone_into(&mut defaults.permission),
            _ => warnings.push(Warning::new(dotted, "unknown key, ignored")),
        }
    }
}

/// `[section] key = ["…", …]`: the strings `accept` takes; everything else
/// is a warning.
fn read_list(
    value: &Value,
    section: &str,
    key: &str,
    warnings: &mut Vec<Warning>,
    accept: impl Fn(&str) -> Result<(), String>,
) -> Vec<String> {
    let mut out = Vec::new();
    let Some(table) = value.as_table() else {
        warnings.push(Warning::new(section, "must be a table, ignored"));
        return out;
    };
    let dotted = format!("{section}.{key}");
    for (name, value) in table {
        if name != key {
            warnings.push(Warning::new(
                format!("{section}.{name}"),
                "unknown key, ignored",
            ));
            continue;
        }
        let Some(items) = value.as_array() else {
            warnings.push(Warning::new(&dotted, "must be a list, ignored"));
            continue;
        };
        for item in items {
            match item.as_str().map(|text| (text, accept(text))) {
                Some((text, Ok(()))) => out.push(text.to_owned()),
                Some((_, Err(message))) => warnings.push(Warning::new(&dotted, message)),
                None => warnings.push(Warning::new(&dotted, "entries must be strings")),
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use leon_domain::{BackendKind, ConversationDefaults};

    use super::{load, parse};

    #[test]
    fn empty_file_gives_defaults() {
        let loaded = parse("");
        assert_eq!(loaded.settings.defaults, ConversationDefaults::default());
        assert_eq!(loaded.warnings, Vec::<super::Warning>::new());
    }

    #[test]
    fn reads_every_default() {
        let loaded = parse(include_str!("../tests/fixtures/config-v1.toml"));
        assert_eq!(loaded.warnings, Vec::<super::Warning>::new());
        let d = loaded.settings.defaults;
        assert_eq!(d.backend, BackendKind::Codex);
        assert_eq!(d.agent.as_deref(), Some("planner"));
        assert_eq!(d.model.as_deref(), Some("gpt-5.5-codex"));
        assert_eq!(d.effort.as_deref(), Some("high"));
        assert_eq!(d.permission, "on-request");
        assert_eq!(loaded.settings.sources, ["~/agent-kit"]);
        assert_eq!(
            loaded.settings.cli_order,
            [BackendKind::Codex, BackendKind::ClaudeCode]
        );
        assert_eq!(
            loaded.settings.diff_fold,
            ["glob:**/*.lock", "re:^generated/"]
        );
        assert_eq!(loaded.settings.zoom, leon_domain::Zoom(125));
        assert_eq!(loaded.settings.open_on, leon_domain::OpenOn::Conversation);
    }

    #[test]
    fn problems_warn_and_keep_defaults() {
        let cases: &[(&str, &str)] = &[
            ("this is = = not toml", ""),
            ("version = 2", "version"),
            ("version = \"one\"", "version"),
            ("theme = \"dark\"", "theme"),
            ("defaults = 3", "defaults"),
            ("[defaults]\nbackend = \"gemini\"", "defaults.backend"),
            ("[defaults]\nmodel = 4", "defaults.model"),
            ("[defaults]\ncolour = \"blue\"", "defaults.colour"),
            ("[sources]\nfolders = \"~/kit\"", "sources.folders"),
            ("[sources]\nfolders = [\"\"]", "sources.folders"),
            ("[sources]\npaths = []", "sources.paths"),
            ("[agents]\ncli_order = [\"gemini\"]", "agents.cli_order"),
            ("[ui]\nzoom = 400", "ui.zoom"),
            ("[ui]\nzoom = \"big\"", "ui.zoom"),
            ("[ui]\nopen_flows_on = \"graph\"", "ui.open_flows_on"),
            ("[ui]\neditor = \"emacs\"", "ui.editor"),
            ("[ui]\nfont = \"Inter\"", "ui.font"),
        ];
        for (text, key) in cases {
            let loaded = parse(text);
            assert_eq!(loaded.warnings.len(), 1, "{text}: {:?}", loaded.warnings);
            assert_eq!(loaded.warnings[0].key, *key, "{text}");
            assert_eq!(
                loaded.settings.defaults,
                ConversationDefaults::default(),
                "{text}"
            );
        }
    }

    #[test]
    fn reads_fold_patterns_and_rejects_bad_ones() {
        let loaded = parse("[diff]\nfold = [\"glob:**/*.lock\", \"re:(\", \"re:^generated/\"]");
        assert_eq!(
            loaded.settings.diff_fold,
            ["glob:**/*.lock", "re:^generated/"]
        );
        assert_eq!(loaded.warnings.len(), 1);
        assert_eq!(loaded.warnings[0].key, "diff.fold");
    }

    #[test]
    fn missing_file_is_silent() {
        let dir = tempfile::tempdir().unwrap();
        let loaded = load(&dir.path().join("config.toml"));
        assert_eq!(loaded.warnings, Vec::<super::Warning>::new());
    }

    #[test]
    fn unreadable_path_warns() {
        let dir = tempfile::tempdir().unwrap();
        let loaded = load(dir.path());
        assert_eq!(loaded.warnings.len(), 1);
    }
}
