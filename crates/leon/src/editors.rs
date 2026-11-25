//! The user's editors (GIT-7): VS Code and Zed, found by their command-line
//! tool on the login shell's `PATH`, or on macOS inside their app, and
//! opened on a folder with a file in it: `code <folder> -g <file>` (VS Code
//! 1.140.0, its `--help`, 2026-11-24); `zed <folder>`, then once it ends
//! `zed <file>` (Zed 1.29.3). Zed puts a file in an open project that holds
//! it, so a worktree inside the main checkout would open there; a folder
//! inside an open project opens as a workspace of its own, and the file
//! then goes to it, the workspace just shown. The CLI ends once Zed opened
//! the paths. From Zed 1.29.3's source (`open_paths`,
//! `visibility_for_path`, `handle_cli_connection`), 2026-11-25.

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

/// The commands' arguments opening `folder`, and `file` in it, run one
/// after the other.
fn commands(editor: Editor, folder: &Path, file: Option<&Path>) -> Vec<Vec<OsString>> {
    let folder = folder.as_os_str().to_owned();
    match (editor, file) {
        (_, None) => vec![vec![folder]],
        (Editor::VsCode, Some(file)) => {
            vec![vec![folder, "-g".into(), file.as_os_str().to_owned()]]
        }
        (Editor::Zed, Some(file)) => vec![vec![folder], vec![file.as_os_str().to_owned()]],
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
        let failed = |e: std::io::Error| PortError::new(CONTEXT, format!("{name}: {e}"));
        let commands = commands(editor, folder, file);
        let Some((last, first)) = commands.split_last() else {
            return Ok(());
        };
        // Waited for: the next one opens in what this one opened.
        for args in first {
            let status = self.command(&program, args).status().map_err(failed)?;
            if !status.success() {
                return Err(PortError::new(CONTEXT, format!("{name}: {status}")));
            }
        }
        let mut child = self.command(&program, last).spawn().map_err(failed)?;
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

    use super::{commands, on_path};

    /// Zed opens the folder first, then the file, so the file goes to the
    /// folder's workspace, not to an open project holding the folder.
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
                vec![os(&[
                    "/code/api/.worktrees/fix",
                    "-g",
                    "/code/api/.worktrees/fix/src/a.rs",
                ])],
            ),
            (
                Editor::Zed,
                Some(file),
                vec![
                    os(&["/code/api/.worktrees/fix"]),
                    os(&["/code/api/.worktrees/fix/src/a.rs"]),
                ],
            ),
            (Editor::Zed, None, vec![os(&["/code/api/.worktrees/fix"])]),
            (
                Editor::VsCode,
                None,
                vec![os(&["/code/api/.worktrees/fix"])],
            ),
        ];
        for (editor, file, expected) in cases {
            assert_eq!(commands(editor, folder, file), expected, "{editor:?}");
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
