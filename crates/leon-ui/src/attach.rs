//! What the user puts in a message besides typing it: long pastes folded
//! into boxes (CONV-12) and images, pasted or dropped on the message box
//! (CONV-18). Each stands in the text as a token, "[Image #1]" for an
//! image; removing the token removes what it stands for.

use std::path::PathBuf;
use std::sync::Arc;

use gpui::{ClipboardEntry, Context, Entity, ExternalPaths, Focusable, ImageFormat, Window};
use gpui_component::input::{InlineToken, InputContent, TextareaState};
use leon_core::ports::{Image, image_mime};

use crate::workspace::Workspace;

/// A message's long pastes and images, by the index in their token's id.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Put {
    pub pasted: Vec<String>,
    pub images: Vec<Image>,
}

impl Put {
    pub(crate) fn is_empty(&self) -> bool {
        self.pasted.is_empty() && self.images.is_empty()
    }
}

/// Ids of images' tokens: this, then the image's index.
const IMAGE_ID: &str = "image-";

/// The token an image stands as, where it was put: "[Image #1]", as the
/// Claude CLI writes it.
fn image_token(ix: usize) -> InlineToken {
    let name = format!("[Image #{}]", ix + 1);
    InlineToken::new(format!("{IMAGE_ID}{ix}"), name.clone()).with_label(name)
}

/// The images whose token is still in the text, in its order.
pub(crate) fn images_in(content: &InputContent, images: &[Image]) -> Vec<Image> {
    content
        .tokens()
        .iter()
        .filter_map(|span| {
            span.token()
                .id()
                .strip_prefix(IMAGE_ID)?
                .parse::<usize>()
                .ok()
        })
        .filter_map(|ix| images.get(ix).cloned())
        .collect()
}

/// A pasted image's file extension: the formats the CLIs take.
fn pasted_ext(format: ImageFormat) -> Option<&'static str> {
    match format {
        ImageFormat::Png => Some("png"),
        ImageFormat::Jpeg => Some("jpg"),
        ImageFormat::Gif => Some("gif"),
        ImageFormat::Webp => Some("webp"),
        _ => None,
    }
}

/// The image files among `paths`; any other file is left out.
fn image_files(paths: &[PathBuf]) -> Vec<Image> {
    paths
        .iter()
        .filter(|p| image_mime(p).is_some())
        .map(|p| Image::File(p.clone()))
        .collect()
}

/// The images a clipboard's content holds: an image, or image files copied.
pub(crate) fn clipboard_images(entries: &[ClipboardEntry]) -> Vec<Image> {
    entries
        .iter()
        .flat_map(|entry| match entry {
            ClipboardEntry::Image(image) => pasted_ext(image.format)
                .map(|ext| Image::Pasted {
                    bytes: Arc::from(image.bytes.as_slice()),
                    ext,
                })
                .into_iter()
                .collect(),
            ClipboardEntry::ExternalPaths(paths) => image_files(paths.paths()),
            ClipboardEntry::String(_) => Vec::new(),
        })
        .collect()
}

impl Workspace {
    /// Puts `images` in `input` where the caret is, each as its token.
    pub(crate) fn put_images(
        &mut self,
        input: &Entity<TextareaState>,
        images: Vec<Image>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        for image in images {
            let token = image_token(self.put.images.len());
            match input.update(cx, |state, cx| state.replace_with_token(token, window, cx)) {
                Ok(()) => self.put.images.push(image),
                Err(error) => tracing::debug!(?error, "could not put an image in the message"),
            }
        }
        cx.notify();
    }

    /// Files dropped on `input`'s box: its images go in the message.
    pub(crate) fn on_drop(
        &mut self,
        input: &Entity<TextareaState>,
        paths: &ExternalPaths,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let images = image_files(paths.paths());
        if !images.is_empty() {
            input.focus_handle(cx).focus(window, cx);
            self.put_images(input, images, window, cx);
        }
    }
}

/// Whether `path` names an image the CLIs take.
#[cfg(test)]
fn is_image(path: &std::path::Path) -> bool {
    image_mime(path).is_some()
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::sync::Arc;

    use gpui::{ClipboardEntry, ExternalPaths, Image as GpuiImage, ImageFormat};
    use gpui_component::input::InputContent;
    use leon_core::ports::Image;

    use super::{clipboard_images, image_files, image_token, images_in, is_image};

    #[test]
    fn only_image_files_go_in() {
        let dropped = [
            PathBuf::from("/Users/me/shot.png"),
            PathBuf::from("/Users/me/notes.txt"),
            PathBuf::from("/Users/me/photo.JPEG"),
        ];
        assert_eq!(
            image_files(&dropped),
            [
                Image::File(dropped[0].clone()),
                Image::File(dropped[2].clone())
            ]
        );
        assert!(!is_image(Path::new("logo.svg")));
    }

    /// A pasted screenshot is one image; copied files give their images; a
    /// text gives none.
    #[test]
    fn a_paste_gives_its_images() {
        let screenshot = ClipboardEntry::Image(GpuiImage {
            format: ImageFormat::Png,
            bytes: b"png".to_vec(),
            id: 1,
        });
        let svg = ClipboardEntry::Image(GpuiImage {
            format: ImageFormat::Svg,
            bytes: b"<svg/>".to_vec(),
            id: 2,
        });
        let files = ClipboardEntry::ExternalPaths(ExternalPaths(
            vec![PathBuf::from("/a.gif"), PathBuf::from("/b.md")].into(),
        ));
        assert_eq!(
            clipboard_images(&[screenshot, svg, files]),
            [
                Image::Pasted {
                    bytes: Arc::from(&b"png"[..]),
                    ext: "png"
                },
                Image::File(PathBuf::from("/a.gif")),
            ]
        );
        let text = ClipboardEntry::String(gpui::ClipboardString::new("hi".into()));
        assert_eq!(clipboard_images(&[text]), []);
    }

    /// Images go in their tokens' order; one whose token was removed stays
    /// out.
    #[test]
    fn images_follow_their_tokens() {
        let images = [
            Image::File(PathBuf::from("/a.png")),
            Image::File(PathBuf::from("/b.png")),
            Image::File(PathBuf::from("/c.png")),
        ];
        let (second, first) = (image_token(1), image_token(0));
        let text = format!("{} then {}", second.text(), first.text());
        let at = text.find(first.text().as_ref()).unwrap();
        let content = InputContent::new(text.clone())
            .with_token(0..second.text().len(), second.clone())
            .unwrap()
            .with_token(at..at + first.text().len(), first)
            .unwrap();
        assert_eq!(
            images_in(&content, &images),
            [images[1].clone(), images[0].clone()]
        );
        assert_eq!(second.text(), "[Image #2]");
    }
}
