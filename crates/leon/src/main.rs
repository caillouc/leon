//! Leon's entry point: builds the adapters, starts the core on tokio (P8) and
//! hands the UI a handle on it. No logic lives here.

mod about;
mod editors;
mod update;

use std::ffi::OsString;
use std::sync::{Arc, OnceLock};
use std::time::Instant;

use anyhow::Context as _;
use leon_domain::Settings;
use tracing_subscriber::EnvFilter;

fn main() -> anyhow::Result<()> {
    let started = Instant::now();
    init_tracing();
    // A coding CLI inside Leon starts `leon mcp` as its MCP server (P10).
    if std::env::args().nth(1).as_deref() == Some("mcp") {
        return leon_app_tools::bridge().context("running the MCP bridge for Leon's tools");
    }
    // Leon runs each CLI under `leon hold <dir> -- <cli> <args…>`, which
    // outlives Leon (I24).
    if std::env::args().nth(1).as_deref() == Some("hold") {
        return hold();
    }
    let (settings, config, config_warnings) = load_settings();
    // Leon's shared MCP list (I14) sits next to the config file.
    let shared_tools = config
        .as_ref()
        .and_then(|c| c.parent())
        .map(|dir| dir.join("mcp.json"));
    let data = data_dir()?;
    let store = open_store(&data)?;
    let socket = data.join("tools.sock");
    let exe = std::env::current_exe().context("finding Leon's own program")?;
    let user_config = leon_config::UserConfig::new(config.clone());
    let about = about::about(about::Facts {
        config,
        config_warnings,
        data: data.clone(),
        exe: exe.clone(),
        login_path,
    });
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_name("leon-core")
        .build()
        .context("starting the core runtime")?;
    let home = dirs::home_dir().context("no home folder for this user")?;
    let sources = settings
        .sources
        .iter()
        .map(|folder| leon_config::expand_home(folder, &home))
        .collect();
    let ports = leon_core::Ports {
        storage: Arc::new(store),
        backends: Arc::new(
            leon_backends::Clis::new(login_path)
                .with_skills_folder(data.join("skills"))
                .with_holders(exe.clone(), data.join("runs"))
                .with_home(home.clone())
                .with_images(data.join("images")),
        ),
        catalog: Arc::new(leon_catalog::AgentFiles::new(home, sources)),
        git: Arc::new(leon_git::GitCli::new(login_path)),
        github: Arc::new(leon_github::GhCli::new(login_path)),
        app_tools: Some(Arc::new(leon_app_tools::LocalTools::new(
            exe.clone(),
            socket.clone(),
        ))),
        shared_tools: Arc::new(leon_catalog::SharedFile::new(
            shared_tools.unwrap_or_else(|| data.join("mcp.json")),
        )),
        config: Arc::new(user_config),
        clock: now_ms,
        about,
        updater: Arc::new(update::SelfUpdate::new(&exe, &data)),
        editors: Arc::new(editors::LocalEditors::new(login_path)),
    };
    let core = leon_core::start(runtime.handle(), ports, settings);
    match leon_app_tools::serve(runtime.handle(), &socket, core.app_tools()) {
        Ok(()) => {}
        Err(error @ leon_app_tools::Error::InUse { .. }) => {
            return Err(error).context("starting Leon");
        }
        Err(error) => tracing::error!(%error, "agents cannot start other agents"),
    }
    // Only now: the doctor checks Leon's tools listen.
    core.send(leon_core::Intent::RunDoctor);
    leon_ui::run(core, started);
    Ok(())
}

/// `leon hold <dir> -- <cli> <args…>`: runs the CLI as its holder until it
/// ends (I24).
fn hold() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(2).collect();
    let (Some(dir), Some("--")) = (args.first(), args.get(1).map(String::as_str)) else {
        anyhow::bail!("usage: leon hold <dir> -- <cli> <args…>");
    };
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("starting the holder's runtime")?
        .block_on(leon_backends::hold(std::path::Path::new(dir), &args[2..]))
        .context("holding a CLI")
}

/// The user's shell `PATH`, read once on first use. An app started from
/// the Finder inherits a minimal `PATH` that misses where `git`, `gh` and
/// the coding CLIs usually live. Read from an interactive login shell:
/// many setups add to `PATH` only in `~/.zshrc` or `~/.bashrc` (Claude
/// Code's installer adds `~/.local/bin` there; seen on 2026-11-18); a plain
/// login shell is the fallback.
fn login_path() -> Option<&'static OsString> {
    static PATH: OnceLock<Option<OsString>> = OnceLock::new();
    PATH.get_or_init(|| {
        let shell = std::env::var_os("SHELL")?;
        let path = shell_path(&shell, "-ilc").or_else(|| shell_path(&shell, "-lc"))?;
        Some(OsString::from(path))
    })
    .as_ref()
}

/// Marks the `PATH` in a shell's output, which may also hold what its
/// startup files print.
const MARK: &str = "__LEON_PATH__";

/// `PATH` as `shell` with `flags` sees it, or `None` when it fails or takes
/// more than a few seconds (a startup file waiting for input).
fn shell_path(shell: &std::ffi::OsStr, flags: &str) -> Option<String> {
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};
    let mut child = Command::new(shell)
        .args([flags, &format!("printf '{MARK}%s{MARK}' \"$PATH\"")])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let deadline = Instant::now() + Duration::from_secs(5);
    while child.try_wait().ok()?.is_none() {
        if Instant::now() > deadline {
            let _ = child.kill();
            return None;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let out = child.wait_with_output().ok()?;
    marked_path(&String::from_utf8_lossy(&out.stdout))
}

/// The `PATH` between the marks, when not empty.
fn marked_path(out: &str) -> Option<String> {
    let (_, rest) = out.split_once(MARK)?;
    let (path, _) = rest.split_once(MARK)?;
    (!path.trim().is_empty()).then(|| path.trim().to_owned())
}

fn now_ms() -> i64 {
    let since = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    i64::try_from(since.as_millis()).unwrap_or(i64::MAX)
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .init();
}

/// User settings (P9), the config file's path and its problems. Problems are
/// logged too; the app always starts.
fn load_settings() -> (Settings, Option<std::path::PathBuf>, Vec<String>) {
    let Some(path) = leon_config::user_config_path() else {
        tracing::warn!("no home directory: using default settings");
        return (Settings::default(), None, Vec::new());
    };
    let loaded = leon_config::load(&path);
    for warning in &loaded.warnings {
        tracing::warn!(file = %path.display(), "config: {warning}");
    }
    let warnings = loaded.warnings.iter().map(ToString::to_string).collect();
    (loaded.settings, Some(path), warnings)
}

/// App data lives in the platform data folder (P9): `~/Library/Application
/// Support/leon` on macOS, `$XDG_DATA_HOME/leon` on Linux.
fn data_dir() -> anyhow::Result<std::path::PathBuf> {
    let dir = dirs::data_dir()
        .context("no data folder for this user")?
        .join("leon");
    std::fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
    Ok(dir)
}

fn open_store(dir: &std::path::Path) -> anyhow::Result<leon_store::Store> {
    let path = dir.join("state.db");
    leon_store::Store::open(&path).with_context(|| format!("opening {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::marked_path;

    #[test]
    fn the_path_is_read_between_marks_whatever_the_shell_prints() {
        let cases = [
            ("__LEON_PATH__/a:/b__LEON_PATH__", Some("/a:/b")),
            (
                "Welcome!\n__LEON_PATH__/a:/b__LEON_PATH__\nbye",
                Some("/a:/b"),
            ),
            ("__LEON_PATH____LEON_PATH__", None),
            ("/a:/b", None),
        ];
        for (out, expected) in cases {
            assert_eq!(marked_path(out).as_deref(), expected, "{out:?}");
        }
    }
}
