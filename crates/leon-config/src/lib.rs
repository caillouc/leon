//! Reads Leon's TOML config files (P9) into [`leon_domain::Settings`], and
//! writes the user's own when Settings change it ([`UserConfig`]).
//!
//! A bad file never stops the app: each problem becomes a [`Warning`] and the
//! default value is used for that key. Unknown keys warn and are ignored.
//!
//! Must not: rewrite a file it cannot parse, drop what it does not know, or
//! know about the UI.

mod parse;
mod paths;
mod write;

pub use parse::{CURRENT_VERSION, Loaded, Warning, load, parse};
pub use paths::{expand_home, user_config_path, user_config_path_from};
pub use write::{SaveError, UserConfig, save, update};
