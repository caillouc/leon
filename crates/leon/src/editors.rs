//! The user's editors (GIT-7): VS Code and Zed, found by their command-line
//! tool on the login shell's `PATH`, or on macOS inside their app, and
//! opened on a folder with a file in it: `code <folder> -g <file>` (VS Code
//! 1.140.0) and `zed <folder> <file>` (Zed 1.29.3), both from their
//! `--help`, 2026-11-24.

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use leon_core::ports::{Editors, PortError, SearchPath};
use leon_domain::Editor;

/// [`Editors`] on this machine, looked up with the `PATH` that `path` gives.
pub(crate) struct LocalEditors {
    path: SearchPath,
}

impl LocalEditors {
    pub(crate) fn new(path: SearchPath) -> Self {
        Self { path }
    }

    /// The editor's command-line tool, if installed.
    fn program(&self, editor: Editor) -> Option<PathBuf> {
        let search = (self.path)()
            .cloned()
            .or_else(|| std::env::var_os("PATH"))
            .unwrap_or_default();
        let (names, apps) = places(editor);
        names
            .iter()
            .find_map(|name| on_path(&search, name))
            .or_else(|| {
                cfg!(target_os = "macos")
                    .then(|| apps.iter().map(PathBuf::from).find(|p| runnable(p)))
                    .flatten()
            })
    }
}

/// Where an editor's tool is: its names on `PATH`, then its path inside its
/// macOS app.
fn places(editor: Editor) -> (&'static [&'static str], &'static [&'static str]) {
    match editor {
        Editor::VsCode => (
            &["code"],
            &["/Applications/Visual Studio Code.app/Contents/Resources/app/bin/code"],
        ),
        // `zeditor`: its name in some Linux packages.
        Editor::Zed => (
            &["zed", "zeditor"],
            &["/Applications/Zed.app/Contents/MacOS/cli"],
        ),
    }
}

/// `name` in one of `search`'s folders, runnable.
fn on_path(search: &OsStr, name: &str) -> Option<PathBuf> {
    std::env::split_paths(search)
        .map(|dir| dir.join(name))
        .find(|p| runnable(p))
}

fn runnable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt as _;
    std::fs::metadata(path).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

/// The arguments opening `folder`, and `file` in it.
fn args(editor: Editor, folder: &Path, file: Option<&Path>) -> Vec<OsString> {
    let mut args = vec![folder.as_os_str().to_owned()];
    if let Some(file) = file {
        if editor == Editor::VsCode {
            args.push("-g".into());
        }
        args.push(file.as_os_str().to_owned());
    }
    args
}

impl Editors for LocalEditors {
    fn installed(&self) -> Vec<Editor> {
        Editor::ALL
            .into_iter()
            .filter(|e| self.program(*e).is_some())
            .collect()
    }

    fn open(&self, editor: Editor, folder: &Path, file: Option<&Path>) -> Result<(), PortError> {
        const CONTEXT: &str = "opening the editor";
        let program = self.program(editor).ok_or_else(|| {
            PortError::new(
                CONTEXT,
                format!("{} is not installed", editor.display_name()),
            )
        })?;
        let mut command = Command::new(&program);
        command
            .args(args(editor, folder, file))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        if let Some(path) = (self.path)() {
            command.env("PATH", path);
        }
        let mut child = command
            .spawn()
            .map_err(|e| PortError::new(CONTEXT, format!("{}: {e}", editor.display_name())))?;
        // The tool hands over to the editor and ends: reaped meanwhile.
        std::thread::spawn(move || child.wait());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;
    use std::path::{Path, PathBuf};

    use leon_domain::Editor;

    use super::{args, on_path};

    #[test]
    fn a_folder_opens_with_its_file() {
        let (folder, file) = (Path::new("/code/api"), Path::new("/code/api/src/a.rs"));
        let os = |items: &[&str]| items.iter().map(OsString::from).collect::<Vec<_>>();
        let cases = [
            (
                Editor::VsCode,
                Some(file),
                os(&["/code/api", "-g", "/code/api/src/a.rs"]),
            ),
            (
                Editor::Zed,
                Some(file),
                os(&["/code/api", "/code/api/src/a.rs"]),
            ),
            (Editor::Zed, None, os(&["/code/api"])),
            (Editor::VsCode, None, os(&["/code/api"])),
        ];
        for (editor, file, expected) in cases {
            assert_eq!(args(editor, folder, file), expected, "{editor:?}");
        }
    }

    /// Found in a `PATH` folder when runnable; a plain file is not a tool.
    #[test]
    fn an_editor_is_found_on_the_path() {
        use std::os::unix::fs::PermissionsExt as _;
        let dir = tempfile::tempdir().unwrap();
        let zed = dir.path().join("zed");
        std::fs::write(&zed, "#!/bin/sh\n").unwrap();
        std::fs::set_permissions(&zed, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::fs::write(dir.path().join("code"), "not runnable").unwrap();
        let search =
            std::env::join_paths([PathBuf::from("/nowhere"), dir.path().to_owned()]).unwrap();
        assert_eq!(on_path(&search, "zed"), Some(zed));
        assert_eq!(on_path(&search, "code"), None);
    }
}
