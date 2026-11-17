//! The small part of YAML frontmatter agent files use: `key: value` lines,
//! optional quotes, and folded (`>`) or literal (`|`) multi-line values.

use std::collections::HashMap;

/// Splits a file into its frontmatter fields and its body.
pub(crate) fn split(text: &str) -> (HashMap<String, String>, &str) {
    let Some(rest) = text
        .strip_prefix("---\n")
        .or_else(|| text.strip_prefix("---\r\n"))
    else {
        return (HashMap::new(), text);
    };
    let Some(end) = rest.find("\n---") else {
        return (HashMap::new(), text);
    };
    let header = &rest[..end];
    let body = rest[end + 4..].trim_start_matches(['\r', '\n', '-']);
    (fields(header), body)
}

fn fields(header: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    let mut lines = header.lines().peekable();
    while let Some(line) = lines.next() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        if line.starts_with([' ', '\t', '-']) {
            continue;
        }
        let value = value.trim();
        let value = if value == ">" || value == "|" || value == ">-" || value == "|-" {
            let mut parts = Vec::new();
            while let Some(next) = lines.peek().filter(|l| l.starts_with([' ', '\t'])) {
                parts.push(next.trim());
                lines.next();
            }
            parts.join(if value.starts_with('>') { " " } else { "\n" })
        } else {
            value.trim_matches(['"', '\'']).to_owned()
        };
        out.insert(key.trim().to_owned(), value);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::split;

    #[test]
    fn reads_simple_and_folded_values() {
        let text = "---\nname: \"coder\"\ndescription: >\n  Writes code\n  carefully.\ntools: Bash, Read\nskills:\n  - map\n---\n\nBody text.\n";
        let (fields, body) = split(text);
        assert_eq!(fields["name"], "coder");
        assert_eq!(fields["description"], "Writes code carefully.");
        assert_eq!(fields["tools"], "Bash, Read");
        assert_eq!(body, "Body text.\n");
    }

    #[test]
    fn no_frontmatter_is_all_body() {
        let (fields, body) = split("Just text");
        assert!(fields.is_empty());
        assert_eq!(body, "Just text");
    }
}
