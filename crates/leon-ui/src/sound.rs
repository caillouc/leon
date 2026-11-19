//! A short sound with desktop notifications (NOTIF-1). GPUI's notifications
//! carry none, so the system's own player plays a system sound, on a thread
//! of its own: the UI never waits for it.

/// The system's player and sound: `afplay` and the system sounds ship with
/// macOS (checked on macOS 26.6.2, 2026-11-19). Elsewhere, none yet.
fn player() -> Option<(&'static str, &'static str)> {
    cfg!(target_os = "macos").then_some(("afplay", "/System/Library/Sounds/Glass.aiff"))
}

/// Plays the notification sound, once, in the background.
pub(crate) fn play() {
    let Some((program, sound)) = player() else {
        return;
    };
    std::thread::spawn(move || {
        if let Err(error) = std::process::Command::new(program).arg(sound).status() {
            tracing::debug!(%error, "playing the notification sound");
        }
    });
}

#[cfg(test)]
mod tests {
    /// The sound macOS ships is where Leon looks for it.
    #[cfg(target_os = "macos")]
    #[test]
    fn the_sound_is_there() {
        let (_, sound) = super::player().unwrap();
        assert!(std::path::Path::new(sound).is_file(), "{sound}");
    }
}
