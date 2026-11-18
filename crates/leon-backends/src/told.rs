//! What an agent is told besides the user's messages, put together the same
//! way for every CLI: the app's rules for its tools (CTX-3), the user's
//! shared instructions (EXT-8), the agent's own.

/// The non-empty parts, in order, joined by a blank line; `None` when every
/// part is empty.
pub(crate) fn told<'a>(parts: impl IntoIterator<Item = Option<&'a str>>) -> Option<String> {
    let parts: Vec<&str> = parts
        .into_iter()
        .flatten()
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .collect();
    (!parts.is_empty()).then(|| parts.join("\n\n"))
}

#[cfg(test)]
mod tests {
    use super::told;

    #[test]
    fn empty_parts_are_left_out() {
        let cases = [
            (
                vec![Some("Use Leon."), None, Some(" Be brief. ")],
                Some("Use Leon.\n\nBe brief."),
            ),
            (vec![Some("  "), None], None),
            (vec![], None),
        ];
        for (parts, expected) in cases {
            assert_eq!(told(parts.clone()).as_deref(), expected, "{parts:?}");
        }
    }
}
