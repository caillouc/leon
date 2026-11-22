//! The holder (I24): a small process of Leon's own, `leon hold <dir> --
//! <cli> <args…>`, that runs one conversation's CLI and outlives Leon. It
//! keeps the CLI's input open (Codex abandons its turn when its input
//! closes), appends what the CLI prints to `<dir>/out` and `<dir>/err`,
//! takes Leon's lines on the socket `<dir>/sock`, and writes `<dir>/exit`
//! once the CLI has ended and all its output is written.
//!
//! On the socket, a line `>text` goes to the CLI's input as `text`; the line
//! `!stop` ends the CLI. When Leon goes away (its connection ends) between
//! turns — no `busy` mark — the holder ends the CLI and itself: only working
//! agents outlive Leon.

use std::path::Path;
use std::process::Stdio;

use tokio::fs::{File, OpenOptions};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixListener;
use tokio::sync::mpsc::{UnboundedSender, unbounded_channel};

/// The files of a holder's folder.
pub(crate) const OUT: &str = "out";
pub(crate) const ERR: &str = "err";
pub(crate) const SOCK: &str = "sock";
pub(crate) const EXIT: &str = "exit";
/// Leon's marks: a turn runs; where in `out` it started.
pub(crate) const BUSY: &str = "busy";
pub(crate) const TURN: &str = "turn";

/// What Leon asks of the CLI.
enum Ask {
    Line(String),
    Stop,
}

/// Runs `cli` (its program, then its arguments) until it ends, as the
/// holder in `dir`. Its environment and folder are the holder's.
///
/// # Errors
/// When the folder, the socket or the CLI cannot be set up.
pub async fn hold(dir: &Path, cli: &[String]) -> std::io::Result<()> {
    let Some((program, args)) = cli.split_first() else {
        return Err(std::io::Error::other("no CLI to run"));
    };
    tokio::fs::create_dir_all(dir).await?;
    let mut child = tokio::process::Command::new(program)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let (Some(mut stdin), Some(stdout), Some(stderr)) =
        (child.stdin.take(), child.stdout.take(), child.stderr.take())
    else {
        return Err(std::io::Error::other("no stdio pipes"));
    };
    // The output files exist before the socket: once Leon can connect, it
    // can read.
    let out = tokio::spawn(append(stdout, opened(&dir.join(OUT)).await?));
    let err = tokio::spawn(append(stderr, opened(&dir.join(ERR)).await?));
    let sock = dir.join(SOCK);
    let _ = tokio::fs::remove_file(&sock).await;
    let listener = UnixListener::bind(&sock)?;
    let (asks, mut asked) = unbounded_channel();
    let accepting = tokio::spawn(accept(listener, asks, dir.to_owned()));
    let status = loop {
        tokio::select! {
            status = child.wait() => break status,
            ask = asked.recv() => match ask {
                Some(Ask::Line(line)) => {
                    let line = format!("{line}\n");
                    if stdin.write_all(line.as_bytes()).await.is_err() {
                        let _ = child.start_kill();
                    }
                    let _ = stdin.flush().await;
                }
                Some(Ask::Stop) => {
                    let _ = child.start_kill();
                }
                None => {}
            },
        }
    };
    accepting.abort();
    let _ = tokio::fs::remove_file(&sock).await;
    // The exit mark comes last: once it is there, the output is whole.
    let _ = out.await;
    let _ = err.await;
    let code = status
        .ok()
        .and_then(|s| s.code())
        .map_or_else(|| "signal".to_owned(), |c| c.to_string());
    tokio::fs::write(dir.join(EXIT), code).await
}

/// `path`, created if needed, to append to.
async fn opened(path: &Path) -> std::io::Result<File> {
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .await
}

/// Appends everything `from` prints to `file`, as it comes.
async fn append(mut from: impl AsyncRead + Unpin, mut file: File) {
    let mut buf = vec![0; 64 * 1024];
    while let Ok(n) = from.read(&mut buf).await {
        if n == 0 || file.write_all(&buf[..n]).await.is_err() {
            break;
        }
        let _ = file.flush().await;
    }
}

/// Takes Leon's connections, one after another as Leon comes back. Once
/// the last one ends with no turn running, the CLI is ended.
async fn accept(listener: UnixListener, asks: UnboundedSender<Ask>, dir: std::path::PathBuf) {
    let connected = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    while let Ok((stream, _)) = listener.accept().await {
        let (asks, connected, dir) = (asks.clone(), connected.clone(), dir.clone());
        connected.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        tokio::spawn(async move {
            let mut lines = BufReader::new(stream).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let ask = if let Some(text) = line.strip_prefix('>') {
                    Ask::Line(text.to_owned())
                } else if line == "!stop" {
                    Ask::Stop
                } else {
                    continue;
                };
                if asks.send(ask).is_err() {
                    return;
                }
            }
            let left = connected.fetch_sub(1, std::sync::atomic::Ordering::SeqCst) - 1;
            if left == 0 && !dir.join(BUSY).exists() {
                let _ = asks.send(Ask::Stop);
            }
        });
    }
}
