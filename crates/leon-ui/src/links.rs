//! What a click on a link in a text opens.

use std::path::Path;

use crate::markdown::Core;

/// A link: a GitHub issue or pull request opens Leon's view of it (GH-2,
/// GH-3); a file on this machine, written as its path, its app (a page an
/// agent made: the browser); anything else the browser.
pub(crate) fn open_link(url: &str, cx: &mut gpui::App) {
    if let Some(path) = local_path(url) {
        cx.open_with_system(path);
        return;
    }
    match (leon_domain::GhRef::from_url(url), cx.try_global::<Core>()) {
        (Some(item), Some(core)) => core.0.send(leon_core::Intent::ShowDetail(Some(item))),
        _ => cx.open_url(url),
    }
}

/// The file a link names by its absolute path, which the system cannot
/// open as a web address.
fn local_path(url: &str) -> Option<&Path> {
    url.starts_with('/').then(|| Path::new(url))
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::local_path;

    #[test]
    fn a_link_to_an_absolute_path_is_a_local_file() {
        let cases = [
            (
                "/Users/me/diagrams/flow.html",
                Some("/Users/me/diagrams/flow.html"),
            ),
            ("https://github.com/acme/api/pull/7", None),
            ("file:///Users/me/flow.html", None),
            ("src/main.rs", None),
            ("#usage", None),
        ];
        for (url, expected) in cases {
            assert_eq!(local_path(url), expected.map(Path::new), "{url}");
        }
    }
}
