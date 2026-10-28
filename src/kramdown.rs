//! Small allowances for Markdown written for kramdown, Jekyll's default.

use std::borrow::Cow;

/// kramdown parses Markdown inside an HTML element marked `markdown="1"`.
/// CommonMark only does so when blank lines separate the tags from the
/// Markdown, so add them.
pub(crate) fn preprocess(markdown: &str) -> Cow<'_, str> {
    if !markdown.contains("markdown=\"1\"") {
        return Cow::Borrowed(markdown);
    }
    let mut out = String::with_capacity(markdown.len() + 64);
    // Names of open `markdown="1"` elements, and how deep each is nested in
    // others of the same name.
    let mut open: Vec<(String, usize)> = Vec::new();
    let mut fence: Option<String> = None;
    for line in markdown.split_inclusive('\n') {
        let trimmed = line.trim();
        if let Some(marker) = &fence {
            if trimmed.starts_with(marker.as_str()) {
                fence = None;
            }
            out.push_str(line);
            continue;
        }
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            fence = Some(trimmed[..3].to_owned());
            out.push_str(line);
            continue;
        }
        if let Some((name, depth)) = open.last_mut() {
            if trimmed.starts_with(&format!("</{name}")) {
                if *depth == 0 {
                    out.push('\n');
                    out.push_str(line);
                    open.pop();
                    continue;
                }
                *depth -= 1;
            } else if trimmed.starts_with(&format!("<{name}")) {
                *depth += 1;
            }
        }
        out.push_str(line);
        if trimmed.starts_with('<') && trimmed.ends_with('>') && trimmed.contains("markdown=\"1\"")
        {
            let name: String = trimmed[1..]
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric())
                .collect();
            if !name.is_empty() {
                if !line.ends_with('\n') {
                    out.push('\n');
                }
                out.push('\n');
                open.push((name, 0));
            }
        }
    }
    Cow::Owned(out)
}

/// Whether `text` is a kramdown attribute list, like `{:.no_toc}`.
pub(crate) fn is_attribute_list(text: &str) -> bool {
    let trimmed = text.trim();
    trimmed.starts_with("{:") && trimmed.ends_with('}')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn separates_markdown_from_tags() {
        let src = "<div markdown=\"1\">\n```rust\n</div>\n```\n<div>\n</div>\n</div>\nafter\n";
        assert_eq!(
            preprocess(src),
            "<div markdown=\"1\">\n\n```rust\n</div>\n```\n<div>\n</div>\n\n</div>\nafter\n"
        );
    }

    #[test]
    fn leaves_other_documents_alone() {
        assert!(matches!(
            preprocess("<div>\n*hi*\n</div>"),
            Cow::Borrowed(_)
        ));
    }
}
