//! Leon's entry point: builds the adapters, starts the core on tokio (P8) and
//! hands the UI a handle on it. No logic lives here.

mod about;

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
        backends: Arc::new(leon_backends::Clis::new(login_path)),
        catalog: Arc::new(leon_catalog::AgentFiles::new(home, sources)),
        git: Arc::new(leon_git::GitCli::new(login_path)),
        github: Arc::new(leon_github::GhCli::new(login_path)),
        app_tools: Some(Arc::new(leon_app_tools::LocalTools::new(
            exe,
            socket.clone(),
        ))),
        shared_tools: Arc::new(leon_catalog::SharedFile::new(
            shared_tools.unwrap_or_else(|| data.join("mcp.json")),
        )),
        config: Arc::new(user_config),
        clock: now_ms,
        about,
    };
    let core = leon_core::start(runtime.handle(), ports, settings);
    match leon_app_tools::serve(runtime.handle(), &socket, core.app_tools()) {
        Ok(()) => {}
        Err(error @ leon_app_tools::Error::InUse { .. }) => {
            return Err(error).context("starting Leon");
        }
        Err(error) => tracing::error!(%error, "agents cannot start other agents"),
    }
    leon_ui::run(core, started);
    Ok(())
}

/// The user's login-shell `PATH`, read once on first use. An app started
/// from the Finder inherits a minimal `PATH` that misses where `git`, `gh`
/// and the coding CLIs usually live.
fn login_path() -> Option<&'static OsString> {
    static PATH: OnceLock<Option<OsString>> = OnceLock::new();
    PATH.get_or_init(|| {
        let shell = std::env::var_os("SHELL")?;
        let out = std::process::Command::new(shell)
            .args(["-l", "-c", "printf %s \"$PATH\""])
            .output()
            .ok()?;
        let path = String::from_utf8(out.stdout).ok()?;
        (!path.trim().is_empty()).then(|| OsString::from(path.trim()))
    })
    .as_ref()
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
