//! Small allowances for Markdown written for kramdown, Jekyll's default.

use std::borrow::Cow;

/// Rewrites what CommonMark would read differently from kramdown.
pub(crate) fn preprocess(markdown: &str) -> Cow<'_, str> {
    let markdown = table_of_contents(markdown);
    if let Cow::Owned(out) = markdown_elements(&markdown) {
        return Cow::Owned(out);
    }
    markdown
}

/// kramdown parses Markdown inside an HTML element marked `markdown="1"`.
/// CommonMark only does so when blank lines separate the tags from the
/// Markdown, so add them.
fn markdown_elements(markdown: &str) -> Cow<'_, str> {
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

/// kramdown replaces a one item list followed by `{:toc}` with a list of
/// links to the document's headings, leaving out any marked `{:.no_toc}`.
/// Write that list out, so it renders as links.
fn table_of_contents(markdown: &str) -> Cow<'_, str> {
    if !markdown.lines().any(is_toc) {
        return Cow::Borrowed(markdown);
    }
    let lines: Vec<&str> = markdown.lines().collect();
    let mut headings = Vec::new();
    let mut ids = Vec::new();
    let mut fence: Option<&str> = None;
    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if let Some(marker) = fence {
            if trimmed.starts_with(marker) {
                fence = None;
            }
            continue;
        }
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            fence = Some(&trimmed[..3]);
            continue;
        }
        let Some((level, text)) = atx_heading(line) else {
            continue;
        };
        let next = lines.get(i + 1).map_or("", |l| l.trim());
        if next.starts_with("{:") && next.contains(".no_toc") {
            continue;
        }
        let mut id = header_id(text);
        let base = id.clone();
        let mut n = 0;
        while ids.contains(&id) {
            n += 1;
            id = format!("{base}-{n}");
        }
        ids.push(id.clone());
        headings.push((level, text, id));
    }

    let mut out = String::with_capacity(markdown.len());
    let mut skip = false;
    for (i, line) in lines.iter().enumerate() {
        if skip {
            skip = false;
            continue;
        }
        let marker = list_marker(line);
        if let Some(marker) = marker
            && lines.get(i + 1).is_some_and(|next| is_toc(next))
        {
            let top = headings.iter().map(|(l, ..)| *l).min().unwrap_or(1);
            let indent = " ".repeat(marker.len() + 1);
            for (level, text, id) in &headings {
                let depth = indent.repeat(level - top);
                out.push_str(&format!("{depth}{marker} [{text}](#{id})\n"));
            }
            skip = true;
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    if !markdown.ends_with('\n') {
        out.pop();
    }
    Cow::Owned(out)
}

/// Whether `line` is kramdown's `{:toc}` marker.
fn is_toc(line: &str) -> bool {
    line.trim()
        .strip_prefix("{:")
        .and_then(|l| l.strip_suffix('}'))
        .is_some_and(|l| l.trim() == "toc")
}

/// The marker of a list item, like `*` or `1.`.
fn list_marker(line: &str) -> Option<&str> {
    let (marker, rest) = line.trim_start().split_once(' ')?;
    let bullet = matches!(marker, "*" | "-" | "+");
    let ordered = marker
        .strip_suffix(['.', ')'])
        .is_some_and(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()));
    ((bullet || ordered) && !rest.trim().is_empty()).then_some(marker)
}

/// The level and text of a `#` heading.
fn atx_heading(line: &str) -> Option<(usize, &str)> {
    let level = line.chars().take_while(|&c| c == '#').count();
    let rest = &line[level..];
    if !(1..=6).contains(&level) || !rest.starts_with([' ', '\t']) {
        return None;
    }
    let mut text = rest.trim();
    // A trailing `{#id}` or attribute list.
    if text.ends_with('}')
        && let Some(start) = text.rfind('{')
    {
        text = text[..start].trim_end();
    }
    let text = text.trim_end_matches('#').trim_end();
    (!text.is_empty()).then_some((level, text))
}

/// The id kramdown gives a heading with GFM input, Jekyll's default: the
/// text in lowercase, keeping only word characters, spaces and dashes, with
/// spaces made dashes.
fn header_id(text: &str) -> String {
    let id: String = text
        .chars()
        .filter(|c| c.is_alphanumeric() || matches!(c, '_' | ' ' | '-'))
        .flat_map(char::to_lowercase)
        .map(|c| if c == ' ' { '-' } else { c })
        .collect();
    if id.is_empty() {
        "section".to_owned()
    } else {
        id
    }
}

/// Drops kramdown attribute lists, like `{:.no_toc}`, from text. They come
/// alone on a line after a block, or right after a span like `` `code` ``,
/// which puts them at the start of the following text.
pub(crate) fn strip_attribute_lists(text: &str) -> &str {
    let mut text = text;
    while let Some(rest) = text.strip_prefix("{:") {
        match rest.find('}') {
            Some(end) => text = &rest[end + 1..],
            None => break,
        }
    }
    text
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
    fn tables_of_contents() {
        let src = "# Title\n{:.no_toc}\n\n* TOC\n{:toc}\n\n## One `a`\n\n```\n# not\n```\n\n### Two\n\n## One `a`\n";
        assert_eq!(
            table_of_contents(src),
            "# Title\n{:.no_toc}\n\n* [One `a`](#one-a)\n  * [Two](#two)\n* [One `a`](#one-a-1)\n\n## One `a`\n\n```\n# not\n```\n\n### Two\n\n## One `a`\n"
        );
        assert_eq!(
            header_id("4. Relating to Recent Posts"),
            "4-relating-to-recent-posts"
        );
        assert!(matches!(table_of_contents("* TOC\n"), Cow::Borrowed(_)));
    }

    #[test]
    fn attribute_lists() {
        assert_eq!(strip_attribute_lists("{:.no_toc}"), "");
        assert_eq!(strip_attribute_lists("{:.a}{: #b}: rest"), ": rest");
        assert_eq!(strip_attribute_lists("text {:.a}"), "text {:.a}");
        assert_eq!(strip_attribute_lists("{:unclosed"), "{:unclosed");
    }

    #[test]
    fn leaves_other_documents_alone() {
        assert!(matches!(
            preprocess("<div>\n*hi*\n</div>"),
            Cow::Borrowed(_)
        ));
    }
}
