//! How Leon talks to a conversation's CLI (I24): through its holder, so the
//! CLI outlives Leon, or — without holders set up (tests) — straight
//! through the process's pipes. Either way: lines in, lines out.
//!
//! A holder's folder also keeps two marks of Leon's: `busy` while a turn
//! runs, and `turn`, where in `out` the current turn starts. Coming back,
//! Leon replays a busy conversation's turn from there.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader, Lines};
use tokio::net::UnixStream;
use tokio::process::{Child, ChildStdin, ChildStdout};
use tokio::task::JoinHandle;

use crate::holder::{BUSY, ERR, EXIT, OUT, SOCK, TURN};
use crate::process::{command, spawn, stderr_tail};

/// How often a quiet holder's output is looked at again.
const POLL: Duration = Duration::from_millis(30);
/// How long a new holder has to open its socket.
const STARTING: Duration = Duration::from_secs(10);

/// Where Leon's holders run from: Leon's own program, and the folder that
/// holds one folder per conversation.
#[derive(Debug, Clone)]
pub(crate) struct Holders {
    pub exe: PathBuf,
    pub runs: PathBuf,
}

impl Holders {
    /// The folder of conversation `key`'s holder.
    pub(crate) fn dir(&self, key: &str) -> PathBuf {
        self.runs.join(key)
    }
}

/// How to start a CLI.
pub(crate) struct Launch<'a> {
    pub path: Option<&'a OsString>,
    pub program: &'a str,
    pub args: &'a [String],
    /// Environment variables added for it.
    pub env: &'a [(String, String)],
    pub cwd: &'a Path,
}

/// A CLI started, or found again, not yet connected to.
pub(crate) enum Opening {
    Direct(Child),
    /// Its holder's folder, and where to start reading its output.
    Held {
        dir: PathBuf,
        from: u64,
    },
}

/// Starts `launch`'s CLI: under a new holder in `holders`' folder for
/// `key` (emptied first), else as Leon's own child.
pub(crate) fn open(
    holders: Option<&Holders>,
    key: &str,
    launch: &Launch<'_>,
) -> std::io::Result<Opening> {
    let Some(holders) = holders else {
        return spawn(
            launch.path,
            launch.program,
            launch.args,
            launch.env,
            launch.cwd,
        )
        .map(Opening::Direct);
    };
    let dir = holders.dir(key);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir)?;
    let mut args = vec![
        "hold".to_owned(),
        dir.display().to_string(),
        "--".to_owned(),
    ];
    args.push(launch.program.to_owned());
    args.extend(launch.args.iter().cloned());
    let mut cmd = command(
        launch.path,
        &holders.exe.display().to_string(),
        &args,
        launch.cwd,
    );
    cmd.envs(launch.env.iter().map(|(k, v)| (k, v)))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    // Its own process group: quitting Leon, or its terminal, leaves it.
    std::os::unix::process::CommandExt::process_group(&mut cmd, 0);
    cmd.spawn()?;
    Ok(Opening::Held { dir, from: 0 })
}

/// Conversation `key`'s holder from before Leon started, if it still runs
/// or its turn was not read to the end; and whether a turn was running.
pub(crate) fn find(holders: &Holders, key: &str) -> Option<(Opening, bool)> {
    let dir = holders.dir(key);
    let runs = dir.join(SOCK).exists() && !dir.join(EXIT).exists();
    let busy = dir.join(BUSY).exists();
    if !runs && !busy {
        let _ = std::fs::remove_dir_all(&dir);
        return None;
    }
    let from = if busy {
        std::fs::read_to_string(dir.join(TURN))
            .ok()
            .and_then(|s| s.trim().parse().ok())
            .unwrap_or(0)
    } else {
        std::fs::metadata(dir.join(OUT)).map_or(0, |m| m.len())
    };
    Some((Opening::Held { dir, from }, busy))
}

/// A turn starts in the holder of `dir`: it is busy, from where its output
/// is now.
pub(crate) fn turn_started(dir: &Path) {
    let at = std::fs::metadata(dir.join(OUT)).map_or(0, |m| m.len());
    let _ = std::fs::write(dir.join(TURN), at.to_string());
    let _ = std::fs::write(dir.join(BUSY), "");
}

/// The turn in the holder of `dir` ended.
pub(crate) fn turn_ended(dir: &Path) {
    let _ = std::fs::remove_file(dir.join(BUSY));
}

impl Opening {
    /// Connects to the CLI: its pipes, or its holder's socket and output.
    pub(crate) async fn connect(self) -> std::io::Result<(Input, Output)> {
        match self {
            Self::Direct(mut child) => {
                let (Some(stdin), Some(stdout), Some(stderr)) =
                    (child.stdin.take(), child.stdout.take(), child.stderr.take())
                else {
                    return Err(std::io::Error::other("no stdio pipes"));
                };
                Ok((
                    Input::Pipe { stdin, child },
                    Output::Pipe {
                        lines: BufReader::new(stdout).lines(),
                        err: stderr_tail(stderr),
                    },
                ))
            }
            Self::Held { dir, from } => {
                let sock = connect(&dir).await;
                let mut out = tokio::fs::File::open(dir.join(OUT)).await?;
                if from > 0 {
                    tokio::io::AsyncSeekExt::seek(&mut out, std::io::SeekFrom::Start(from)).await?;
                }
                let live = sock.is_some();
                Ok((
                    Input::Held(sock),
                    Output::Held {
                        tail: Tail {
                            out,
                            buf: Vec::new(),
                            dir,
                        },
                        live,
                    },
                ))
            }
        }
    }
}

/// The holder's socket once it opens; `None` when the holder is gone (its
/// output can still be read).
async fn connect(dir: &Path) -> Option<UnixStream> {
    let started = std::time::Instant::now();
    loop {
        if let Ok(sock) = UnixStream::connect(dir.join(SOCK)).await {
            return Some(sock);
        }
        if dir.join(EXIT).exists() || started.elapsed() > STARTING {
            return None;
        }
        tokio::time::sleep(POLL).await;
    }
}

/// What Leon sends a connected CLI.
pub(crate) enum Input {
    Pipe {
        stdin: ChildStdin,
        child: Child,
    },
    /// The holder's socket; `None` when it is gone.
    Held(Option<UnixStream>),
}

/// What a connected CLI prints.
pub(crate) enum Output {
    Pipe {
        lines: Lines<BufReader<ChildStdout>>,
        err: JoinHandle<String>,
    },
    /// `live`: the holder runs, so its output may still grow.
    Held { tail: Tail, live: bool },
}

/// A holder's output, read as it grows.
pub(crate) struct Tail {
    out: tokio::fs::File,
    buf: Vec<u8>,
    dir: PathBuf,
}

impl Input {
    /// Sends `line` (without its line break) to the CLI; `false` once it
    /// cannot take it.
    pub(crate) async fn send(&mut self, line: &str) -> bool {
        match self {
            Self::Pipe { stdin, .. } => {
                let line = format!("{line}\n");
                stdin.write_all(line.as_bytes()).await.is_ok() && stdin.flush().await.is_ok()
            }
            Self::Held(Some(sock)) => {
                let line = format!(">{line}\n");
                sock.write_all(line.as_bytes()).await.is_ok()
            }
            Self::Held(None) => false,
        }
    }

    /// Ends the CLI: Leon stopped it.
    pub(crate) async fn stop(&mut self) {
        match self {
            Self::Pipe { child, .. } => {
                let _ = child.start_kill();
            }
            Self::Held(Some(sock)) => {
                let _ = sock.write_all(b"!stop\n").await;
            }
            Self::Held(None) => {}
        }
    }
}

impl Output {
    /// The CLI's next line; `None` once it has ended and all is read.
    pub(crate) async fn next_line(&mut self) -> Option<String> {
        match self {
            Self::Pipe { lines, .. } => lines.next_line().await.ok().flatten(),
            Self::Held { tail, live } => tail.next_line(*live).await,
        }
    }
}

/// Once the CLI has ended: whether it succeeded, and its last lines on
/// stderr.
pub(crate) async fn ended(input: Input, output: Output) -> (bool, String) {
    match (input, output) {
        (Input::Pipe { mut child, .. }, Output::Pipe { err, .. }) => {
            let ok = child.wait().await.is_ok_and(|s| s.success());
            (ok, err.await.unwrap_or_default())
        }
        (_, Output::Held { tail, .. }) => {
            let exit = tokio::fs::read_to_string(tail.dir.join(EXIT)).await;
            let err = tokio::fs::read_to_string(tail.dir.join(ERR)).await;
            let lines: Vec<&str> = err.as_deref().unwrap_or_default().lines().collect();
            let last = lines[lines.len().saturating_sub(12)..].join("\n");
            (exit.is_ok_and(|code| code.trim() == "0"), last)
        }
        (_, Output::Pipe { err, .. }) => (false, err.await.unwrap_or_default()),
    }
}

impl Tail {
    /// The next whole line; at the end, waits for more while the CLI runs
    /// (`live`: its holder is there).
    async fn next_line(&mut self, live: bool) -> Option<String> {
        let mut chunk = vec![0; 64 * 1024];
        loop {
            if let Some(end) = self.buf.iter().position(|b| *b == b'\n') {
                let whole: Vec<u8> = self.buf.drain(..=end).collect();
                return Some(String::from_utf8_lossy(&whole[..end]).into_owned());
            }
            match self.out.read(&mut chunk).await {
                Ok(n) if n > 0 => self.buf.extend_from_slice(&chunk[..n]),
                Ok(_) if live && !self.dir.join(EXIT).exists() => tokio::time::sleep(POLL).await,
                // Ended, or gone: read what is left once more, then stop.
                _ => match self.out.read(&mut chunk).await {
                    Ok(n) if n > 0 => self.buf.extend_from_slice(&chunk[..n]),
                    _ => {
                        let rest = std::mem::take(&mut self.buf);
                        return (!rest.is_empty())
                            .then(|| String::from_utf8_lossy(&rest).into_owned());
                    }
                },
            }
        }
    }
}

#[cfg(test)]
mod tests;
