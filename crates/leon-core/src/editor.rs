//! Files and worktrees opened in the user's editor (GIT-7): the one chosen
//! in Settings when installed, else the first one installed.

use std::path::PathBuf;
use std::sync::Arc;

use crate::app::{Core, blocking};
use crate::repo::Internal;

impl Core {
    /// The editors installed, read at launch.
    pub(crate) fn load_editors(&mut self) {
        let editors = Arc::clone(&self.ports.editors);
        self.state.editors = Arc::new(blocking(|| editors.installed()));
    }

    /// Opens `folder`, and `file` in it when given, in the background; a
    /// failure, or no editor, says so.
    pub(crate) fn open_in_editor(&mut self, folder: PathBuf, file: Option<PathBuf>) {
        let chosen = leon_domain::Editor::pick(self.state.settings.editor, &self.state.editors);
        let Some(editor) = chosen else {
            self.flash("No editor found: install VS Code or Zed".to_owned());
            return;
        };
        let (editors, tx) = (Arc::clone(&self.ports.editors), self.internal_tx.clone());
        crate::jobs::spawn(move || {
            if let Err(error) = editors.open(editor, &folder, file.as_deref()) {
                let _ = tx.send(Internal::Flash(error.to_string()));
            }
        });
    }
}
