//! Writes the user's settings into the config file (SETTINGS-1), editing the
//! file in place: comments, order and keys Leon does not know stay. A key
//! whose value is the default is removed, so the file only holds choices.

use std::path::{Path, PathBuf};

use leon_core::ports::{ConfigFile, PortError};
use leon_domain::{SessionDefaults, Settings};
use toml_edit::{Array, DocumentMut, Item, Table, value};

use crate::CURRENT_VERSION;

/// Why the file could not be written.
#[derive(Debug, thiserror::Error)]
pub enum SaveError {
    /// The file exists but could not be read.
    #[error("cannot read {}", .path.display())]
    Read {
        /// The file.
        path: PathBuf,
        /// Why.
        source: std::io::Error,
    },
    /// The file is not valid TOML, so Leon will not touch it.
    #[error("{} is not valid TOML; fix it first: {message}", .path.display())]
    Invalid {
        /// The file.
        path: PathBuf,
        /// The parser's message.
        message: String,
    },
    /// The new file could not be written.
    #[error("cannot write {}", .path.display())]
    Write {
        /// The file.
        path: PathBuf,
        /// Why.
        source: std::io::Error,
    },
}

/// The [`ConfigFile`] port on the user's config file.
#[derive(Debug, Clone)]
pub struct UserConfig {
    path: Option<PathBuf>,
}

impl UserConfig {
    /// `path`: the config file; `None` without a home folder.
    pub fn new(path: Option<PathBuf>) -> Self {
        Self { path }
    }
}

impl ConfigFile for UserConfig {
    fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    fn save(&self, settings: &Settings) -> Result<(), PortError> {
        let context = "saving the settings";
        let path = self
            .path
            .as_deref()
            .ok_or_else(|| PortError::new(context, "no home folder"))?;
        save(path, settings).map_err(|e| PortError::new(context, error_text(&e)))
    }
}

/// The error and its cause, on one line.
fn error_text(error: &SaveError) -> String {
    match error {
        SaveError::Read { source, .. } | SaveError::Write { source, .. } => {
            format!("{error}: {source}")
        }
        SaveError::Invalid { .. } => error.to_string(),
    }
}

/// Writes `settings` into the file at `path`, creating it and its folder if
/// needed. The new file replaces the old one in one step.
pub fn save(path: &Path, settings: &Settings) -> Result<(), SaveError> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(source) => {
            return Err(SaveError::Read {
                path: path.to_owned(),
                source,
            });
        }
    };
    let text = update(&text, settings).map_err(|message| SaveError::Invalid {
        path: path.to_owned(),
        message,
    })?;
    let write = |source| SaveError::Write {
        path: path.to_owned(),
        source,
    };
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(write)?;
    }
    let temporary = path.with_extension("toml.saving");
    std::fs::write(&temporary, text).map_err(write)?;
    std::fs::rename(&temporary, path).map_err(write)
}

/// The config text with `settings` written into it.
pub fn update(text: &str, settings: &Settings) -> Result<String, String> {
    let mut doc: DocumentMut = text
        .parse()
        .map_err(|e: toml_edit::TomlError| e.message().to_owned())?;
    if !doc.contains_key("version") {
        doc.insert("version", value(CURRENT_VERSION));
    }
    let defaults = SessionDefaults::default();
    let d = &settings.defaults;
    let changed = |is_default: bool, item: Item| (!is_default).then_some(item);
    set(
        &mut doc,
        "defaults",
        "backend",
        changed(d.backend == defaults.backend, value(d.backend.id())),
    );
    set(&mut doc, "defaults", "model", d.model.as_deref().map(value));
    set(
        &mut doc,
        "defaults",
        "effort",
        d.effort.as_deref().map(value),
    );
    set(
        &mut doc,
        "defaults",
        "permission",
        changed(d.permission == defaults.permission, value(&d.permission)),
    );
    set(&mut doc, "diff", "fold", list(&settings.diff_fold));
    set(&mut doc, "sources", "folders", list(&settings.sources));
    Ok(doc.to_string())
}

fn list(items: &[String]) -> Option<Item> {
    (!items.is_empty()).then(|| value(items.iter().collect::<Array>()))
}

/// Sets `section.key`, or removes it (and the section, once empty).
fn set(doc: &mut DocumentMut, section: &str, key: &str, item: Option<Item>) {
    if let Some(item) = item {
        let table = doc
            .entry(section)
            .or_insert_with(|| Item::Table(Table::new()));
        if !table.is_table() {
            *table = Item::Table(Table::new());
        }
        table[key] = item;
    } else if let Some(table) = doc.get_mut(section).and_then(Item::as_table_mut) {
        table.remove(key);
        if table.is_empty() {
            doc.remove(section);
        }
    }
}

#[cfg(test)]
mod tests {
    use leon_domain::{BackendKind, Settings};

    use super::{save, update};
    use crate::parse;

    #[test]
    fn writes_choices_and_keeps_the_rest_of_the_file() {
        let original = "# My Leon config.\nversion = 1\n\n[defaults]\n# Codex for everything.\nbackend = \"codex\"\nmodel = \"gpt-5.5\"\n\n[extra]\nkept = true\n";
        let mut settings = parse(original).settings;
        settings.defaults.model = None;
        settings.defaults.effort = Some("high".into());
        settings.diff_fold = vec!["glob:**/*.lock".into()];
        let text = update(original, &settings).unwrap();
        assert_eq!(
            text,
            "# My Leon config.\nversion = 1\n\n[defaults]\n# Codex for everything.\nbackend = \"codex\"\neffort = \"high\"\n\n[extra]\nkept = true\n\n[diff]\nfold = [\"glob:**/*.lock\"]\n"
        );
        // What was written reads back as what was saved.
        assert_eq!(parse(&text).settings, settings);
    }

    #[test]
    fn defaults_are_left_out() {
        let cases = [
            ("", Settings::default(), "version = 1\n"),
            (
                "version = 1\n\n[defaults]\nbackend = \"codex\"\n",
                Settings::default(),
                "version = 1\n",
            ),
            (
                "",
                Settings {
                    defaults: leon_domain::SessionDefaults {
                        backend: BackendKind::CopilotCli,
                        ..Default::default()
                    },
                    sources: vec!["~/kit".into()],
                    ..Settings::default()
                },
                "version = 1\n\n[defaults]\nbackend = \"copilot\"\n\n[sources]\nfolders = [\"~/kit\"]\n",
            ),
        ];
        for (original, settings, expected) in cases {
            assert_eq!(update(original, &settings).unwrap(), expected, "{original}");
        }
    }

    #[test]
    fn an_invalid_file_is_not_touched() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "this = = not toml").unwrap();
        let error = save(&path, &Settings::default()).unwrap_err();
        assert!(error.to_string().contains("is not valid TOML"), "{error}");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "this = = not toml");
    }

    #[test]
    fn creates_the_file_and_its_folder() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("leon").join("config.toml");
        let settings = Settings {
            sources: vec!["~/kit".into()],
            ..Settings::default()
        };
        save(&path, &settings).unwrap();
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "version = 1\n\n[sources]\nfolders = [\"~/kit\"]\n"
        );
    }
}
