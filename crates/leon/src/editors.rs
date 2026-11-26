//! The user's editors (GIT-7): VS Code and Zed, found by their command-line
//! tool on the login shell's `PATH`, or on macOS inside their app, and
//! opened on a folder with a file in it: `code <folder> -g <file>` (VS Code
//! 1.140.0, its `--help`, 2026-11-24); `zed -e <folder> <file>`, one call,
//! in the window already open (I29). Zed 1.30.2 (its source, 2026-11-26:
//! `find_existing_workspace`, `open_paths`, `visibility_for_path`) shows the
//! project whose root is the folder, else adds the folder as a project of
//! the window and shows it, and opens the file there. Two calls raced: the
//! file came before the folder's project was ready, and Zed put it in the
//! window's current project, the main checkout.

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

/// The editor's arguments opening `folder`, and `file` in it.
fn args(editor: Editor, folder: &Path, file: Option<&Path>) -> Vec<OsString> {
    let folder = folder.as_os_str().to_owned();
    let file = file.map(|f| f.as_os_str().to_owned());
    match editor {
        Editor::VsCode => std::iter::once(folder)
            .chain(file.into_iter().flat_map(|f| [OsString::from("-g"), f]))
            .collect(),
        Editor::Zed => [OsString::from("-e"), folder]
            .into_iter()
            .chain(file)
            .collect(),
    }
}

impl LocalEditors {
    /// The editor's tool with `args`, quiet, on the login shell's `PATH`.
    fn command(&self, program: &Path, args: &[OsString]) -> Command {
        let mut command = Command::new(program);
        command
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        if let Some(path) = (self.path)() {
            command.env("PATH", path);
        }
        command
    }
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
        let name = editor.display_name();
        let mut child = self
            .command(&program, &args(editor, folder, file))
            .spawn()
            .map_err(|e| PortError::new(CONTEXT, format!("{name}: {e}")))?;
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

    /// One call each: Zed gets the folder and its file together, in the
    /// window already open, so the file cannot go to another project.
    #[test]
    fn a_folder_opens_with_its_file() {
        let (folder, file) = (
            Path::new("/code/api/.worktrees/fix"),
            Path::new("/code/api/.worktrees/fix/src/a.rs"),
        );
        let os = |items: &[&str]| items.iter().map(OsString::from).collect::<Vec<_>>();
        let cases = [
            (
                Editor::VsCode,
                Some(file),
                os(&[
                    "/code/api/.worktrees/fix",
                    "-g",
                    "/code/api/.worktrees/fix/src/a.rs",
                ]),
            ),
            (
                Editor::Zed,
                Some(file),
                os(&[
                    "-e",
                    "/code/api/.worktrees/fix",
                    "/code/api/.worktrees/fix/src/a.rs",
                ]),
            ),
            (Editor::Zed, None, os(&["-e", "/code/api/.worktrees/fix"])),
            (Editor::VsCode, None, os(&["/code/api/.worktrees/fix"])),
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
