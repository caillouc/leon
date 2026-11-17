//! Leon's entry point: builds the adapters, starts the core on tokio (P8) and
//! hands the UI a handle on it. No logic lives here.

use std::sync::Arc;
use std::time::Instant;

use anyhow::Context as _;
use leon_domain::Settings;
use tracing_subscriber::EnvFilter;

fn main() -> anyhow::Result<()> {
    let started = Instant::now();
    init_tracing();
    let settings = load_settings();
    let store = open_store()?;
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_name("leon-core")
        .build()
        .context("starting the core runtime")?;
    let home = dirs::home_dir().context("no home folder for this user")?;
    let ports = leon_core::Ports {
        storage: Arc::new(store),
        backends: Arc::new(leon_backends::Clis),
        catalog: Arc::new(leon_catalog::AgentFiles::new(home)),
        clock: now_ms,
    };
    let core = leon_core::start(runtime.handle(), ports, settings);
    leon_ui::run(core, started);
    Ok(())
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

/// User settings (P9). Problems are logged; the app always starts.
fn load_settings() -> Settings {
    let Some(path) = leon_config::user_config_path() else {
        tracing::warn!("no home directory: using default settings");
        return Settings::default();
    };
    let loaded = leon_config::load(&path);
    for warning in &loaded.warnings {
        tracing::warn!(file = %path.display(), "config: {warning}");
    }
    loaded.settings
}

/// App data lives in the platform data folder (P9): `~/Library/Application
/// Support/leon` on macOS, `$XDG_DATA_HOME/leon` on Linux.
fn open_store() -> anyhow::Result<leon_store::Store> {
    let dir = dirs::data_dir()
        .context("no data folder for this user")?
        .join("leon");
    std::fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
    let path = dir.join("state.db");
    leon_store::Store::open(&path).with_context(|| format!("opening {}", path.display()))
}
