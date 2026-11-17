use std::ffi::OsString;
use std::path::PathBuf;

/// The user config file: `$XDG_CONFIG_HOME/leon/config.toml`, else
/// `~/.config/leon/config.toml`, on macOS and Linux alike (P9).
pub fn user_config_path() -> Option<PathBuf> {
    user_config_path_from(std::env::var_os("XDG_CONFIG_HOME"), dirs::home_dir())
}

/// [`user_config_path`] with its inputs given, for tests.
pub fn user_config_path_from(
    xdg_config_home: Option<OsString>,
    home: Option<PathBuf>,
) -> Option<PathBuf> {
    let base = match xdg_config_home.map(PathBuf::from) {
        Some(dir) if dir.is_absolute() => dir,
        _ => home?.join(".config"),
    };
    Some(base.join("leon").join("config.toml"))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::user_config_path_from;

    #[test]
    fn xdg_config_home_wins() {
        let path = user_config_path_from(Some("/xdg".into()), Some(PathBuf::from("/home/u")));
        assert_eq!(path, Some(PathBuf::from("/xdg/leon/config.toml")));
    }

    #[test]
    fn falls_back_to_dot_config() {
        let path = user_config_path_from(None, Some(PathBuf::from("/home/u")));
        assert_eq!(
            path,
            Some(PathBuf::from("/home/u/.config/leon/config.toml"))
        );
    }

    #[test]
    fn relative_xdg_is_ignored() {
        let path = user_config_path_from(Some("rel".into()), Some(PathBuf::from("/home/u")));
        assert_eq!(
            path,
            Some(PathBuf::from("/home/u/.config/leon/config.toml"))
        );
    }

    #[test]
    fn no_home_no_path() {
        assert_eq!(user_config_path_from(None, None), None);
    }
}
