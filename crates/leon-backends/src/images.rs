//! Images in a message (CONV-18, I27): saved with a conversation's files,
//! in `images/<conversation id>/` of Leon's data folder, then given to each
//! CLI as it takes them — base64 for Claude Code and Copilot CLI, the file's
//! path for Codex.

use std::path::{Path, PathBuf};

use base64::Engine as _;
use leon_core::ports::{Image, PortError, image_mime as mime};
use leon_domain::ConversationId;

const CONTEXT: &str = "saving an image";

/// Saves `image` as the next file of conversation `id`'s folder in `root`;
/// one already there (saved while its message waited, I30) stays as it is.
pub(crate) fn save(root: &Path, id: ConversationId, image: &Image) -> Result<PathBuf, PortError> {
    let dir = folder(root, id);
    if let Image::File(file) = image
        && file.parent() == Some(dir.as_path())
    {
        return Ok(file.clone());
    }
    std::fs::create_dir_all(&dir).map_err(|e| PortError::new(CONTEXT, e))?;
    let next = std::fs::read_dir(&dir).map_or(0, Iterator::count) + 1;
    let saved = |ext: &str| dir.join(format!("{next}.{ext}"));
    match image {
        Image::Pasted { bytes, ext } => {
            let path = saved(ext);
            std::fs::write(&path, bytes).map_err(|e| PortError::new(CONTEXT, e))?;
            Ok(path)
        }
        Image::File(file) => {
            mime(file).ok_or_else(|| {
                PortError::new(
                    CONTEXT,
                    format!("{}: not a PNG, JPEG, GIF or WebP image", file.display()),
                )
            })?;
            let ext = file.extension().and_then(|e| e.to_str()).unwrap_or("png");
            let path = saved(&ext.to_ascii_lowercase());
            std::fs::copy(file, &path)
                .map_err(|e| PortError::new(CONTEXT, format!("{}: {e}", file.display())))?;
            Ok(path)
        }
    }
}

/// Conversation `id`'s images.
pub(crate) fn folder(root: &Path, id: ConversationId) -> PathBuf {
    root.join(id.0.to_string())
}

/// An image file's MIME type and bytes in base64; `None`, logged, when it
/// cannot be read.
pub(crate) fn encoded(path: &Path) -> Option<(&'static str, String)> {
    let mime = mime(path)?;
    match std::fs::read(path) {
        Ok(bytes) => Some((
            mime,
            base64::engine::general_purpose::STANDARD.encode(bytes),
        )),
        Err(error) => {
            tracing::warn!(%error, path = %path.display(), "an image of the message could not be read");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::sync::Arc;

    use leon_core::ports::Image;
    use leon_domain::ConversationId;

    use super::{encoded, mime, save};

    #[test]
    fn only_images_the_clis_take_are_sent() {
        let cases = [
            ("shot.PNG", Some("image/png")),
            ("photo.jpeg", Some("image/jpeg")),
            ("a.jpg", Some("image/jpeg")),
            ("anim.gif", Some("image/gif")),
            ("x.webp", Some("image/webp")),
            ("logo.svg", None),
            ("notes.txt", None),
            ("README", None),
        ];
        for (name, expected) in cases {
            assert_eq!(mime(Path::new(name)), expected, "{name}");
        }
    }

    /// Pasted bytes and a dropped file land in the conversation's folder,
    /// numbered; a file that is no image is refused.
    #[test]
    fn images_are_saved_in_the_conversations_folder() {
        let root = tempfile::tempdir().unwrap();
        let dropped = root.path().join("shot.JPG");
        std::fs::write(&dropped, b"jpeg bytes").unwrap();
        let id = ConversationId(7);
        let pasted = Image::Pasted {
            bytes: Arc::from(&b"png bytes"[..]),
            ext: "png",
        };
        let first = save(root.path(), id, &pasted).unwrap();
        let second = save(root.path(), id, &Image::File(dropped)).unwrap();
        assert_eq!(first, root.path().join("7/1.png"));
        assert_eq!(second, root.path().join("7/2.jpg"));
        assert_eq!(std::fs::read(&second).unwrap(), b"jpeg bytes");
        // Saved already: not copied again.
        assert_eq!(
            save(root.path(), id, &Image::File(first.clone())).unwrap(),
            first
        );
        assert_eq!(
            encoded(&first),
            Some(("image/png", "cG5nIGJ5dGVz".to_owned()))
        );
        let text = root.path().join("notes.txt");
        std::fs::write(&text, b"hi").unwrap();
        assert!(save(root.path(), id, &Image::File(text)).is_err());
        assert_eq!(encoded(&PathBuf::from("/nowhere/1.png")), None);
    }
}
