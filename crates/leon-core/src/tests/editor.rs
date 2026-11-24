//! Files and worktrees open in the user's editor (GIT-7).

use std::sync::atomic::{AtomicBool, Ordering};

use leon_domain::Editor;

use super::*;
use crate::ports::Editors;

#[derive(Default)]
pub(super) struct FakeEditors {
    /// Zed is installed too, when set.
    zed: AtomicBool,
    pub(super) opened: Mutex<Vec<(Editor, PathBuf, Option<PathBuf>)>>,
}

impl Editors for FakeEditors {
    fn installed(&self) -> Vec<Editor> {
        let mut found = vec![Editor::VsCode];
        if self.zed.load(Ordering::SeqCst) {
            found.push(Editor::Zed);
        }
        found
    }
    fn open(&self, editor: Editor, folder: &Path, file: Option<&Path>) -> Result<(), PortError> {
        self.opened
            .lock()
            .unwrap()
            .push((editor, folder.to_owned(), file.map(Path::to_owned)));
        Ok(())
    }
}

/// The editor chosen in Settings when installed, else the first installed;
/// a worktree alone, or a file in it.
#[tokio::test(flavor = "multi_thread")]
async fn files_and_worktrees_open_in_the_chosen_editor() {
    for (chosen, zed, expected) in [
        (None, true, Editor::VsCode),
        (Some(Editor::Zed), true, Editor::Zed),
        (Some(Editor::Zed), false, Editor::VsCode),
    ] {
        let editors = Arc::new(FakeEditors::default());
        editors.zed.store(zed, Ordering::SeqCst);
        let core = start(
            &tokio::runtime::Handle::current(),
            Ports {
                editors: Arc::clone(&editors) as Arc<dyn Editors>,
                ..ports(
                    Arc::new(FakeStorage::default()),
                    Arc::new(FakeBackends::default()),
                )
            },
            Settings {
                editor: chosen,
                ..Settings::default()
            },
        );
        until(&core, "loaded", |s| s.loaded).await;
        let worktree = PathBuf::from("/code/api/.worktrees/fix");
        core.send(Intent::OpenInEditor {
            folder: worktree.clone(),
            file: Some(worktree.join("src/retry.rs")),
        });
        core.send(Intent::OpenInEditor {
            folder: worktree.clone(),
            file: None,
        });
        let start = std::time::Instant::now();
        while editors.opened.lock().unwrap().len() < 2 && start.elapsed() < Duration::from_secs(2) {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        let opened = editors.opened.lock().unwrap().clone();
        assert_eq!(opened.len(), 2, "{chosen:?}");
        assert!(opened.contains(&(
            expected,
            worktree.clone(),
            Some(worktree.join("src/retry.rs"))
        )));
        assert!(opened.contains(&(expected, worktree, None)));
    }
}
