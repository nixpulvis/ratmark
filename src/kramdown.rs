//! Small allowances for Markdown written for kramdown, Jekyll's default.

use std::borrow::Cow;

/// Rewrites what CommonMark would read differently from kramdown.
pub(crate) fn preprocess(markdown: &str) -> Cow<'_, str> {
    let markdown = Toc::expand(markdown);
    match MarkdownElements::separate(&markdown) {
        Cow::Owned(out) => Cow::Owned(out),
        Cow::Borrowed(_) => markdown,
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

/// The classes in the kramdown attribute lists at the start of `text`, like
/// `new` and `old` from `{:.new}{: .old #id}`, which belong to the span just
/// before it.
pub(crate) fn span_classes(text: &str) -> Vec<&str> {
    let mut classes = Vec::new();
    let mut text = text;
    while let Some(rest) = text.strip_prefix("{:") {
        let Some(end) = rest.find('}') else { break };
        for word in rest[..end].split_whitespace() {
            if word.starts_with('.') {
                classes.extend(word.split('.').filter(|c| !c.is_empty()));
            }
        }
        text = &rest[end + 1..];
    }
    classes
}

/// kramdown replaces a one item list followed by `{:toc}` with a list of
/// links to the document's headings, leaving out any marked `{:.no_toc}`.
/// This writes that list out, so it renders as links.
struct Toc<'a> {
    entries: Vec<Entry<'a>>,
}

/// A heading listed in a table of contents.
struct Entry<'a> {
    level: usize,
    text: &'a str,
    id: String,
}

impl<'a> Toc<'a> {
    fn expand(markdown: &'a str) -> Cow<'a, str> {
        if !markdown.lines().any(Self::is_marker) {
            return Cow::Borrowed(markdown);
        }
        let lines: Vec<&str> = markdown.lines().collect();
        let toc = Self::collect(&lines);
        let mut out = String::with_capacity(markdown.len());
        let mut lines = lines.iter().peekable();
        while let Some(line) = lines.next() {
            let next = lines.peek().copied().copied();
            if let Some(marker) = Self::placeholder(line, next) {
                toc.write(marker, &mut out);
                lines.next();
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

    /// The document's headings, outside of code, with unique ids.
    fn collect(lines: &[&'a str]) -> Self {
        let mut entries: Vec<Entry> = Vec::new();
        let mut fence = Fence::default();
        for (i, line) in lines.iter().enumerate() {
            if fence.contains(line) {
                continue;
            }
            let Some((level, text)) = Entry::heading(line) else {
                continue;
            };
            let next = lines.get(i + 1).map_or("", |l| l.trim());
            if next.starts_with("{:") && next.contains(".no_toc") {
                continue;
            }
            let base = Entry::header_id(text);
            let mut id = base.clone();
            let mut n = 0;
            while entries.iter().any(|e| e.id == id) {
                n += 1;
                id = format!("{base}-{n}");
            }
            entries.push(Entry { level, text, id });
        }
        Self { entries }
    }

    /// Writes the entries as a nested list of links, with `marker` bullets.
    fn write(&self, marker: &str, out: &mut String) {
        let top = self.entries.iter().map(|e| e.level).min().unwrap_or(1);
        let indent = " ".repeat(marker.len() + 1);
        for Entry { level, text, id } in &self.entries {
            let depth = indent.repeat(level - top);
            out.push_str(&format!("{depth}{marker} [{text}](#{id})\n"));
        }
    }

    /// Whether `line` is kramdown's `{:toc}` marker.
    fn is_marker(line: &str) -> bool {
        line.trim()
            .strip_prefix("{:")
            .and_then(|l| l.strip_suffix('}'))
            .is_some_and(|l| l.trim() == "toc")
    }

    /// The list marker, like `*` or `1.`, of a list item standing in for the
    /// table of contents, which is one followed by the `{:toc}` marker.
    fn placeholder<'l>(line: &'l str, next: Option<&str>) -> Option<&'l str> {
        if !next.is_some_and(Self::is_marker) {
            return None;
        }
        let (marker, rest) = line.trim_start().split_once(' ')?;
        let bullet = matches!(marker, "*" | "-" | "+");
        let ordered = marker
            .strip_suffix(['.', ')'])
            .is_some_and(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()));
        ((bullet || ordered) && !rest.trim().is_empty()).then_some(marker)
    }
}

impl Entry<'_> {
    /// The level and text of a `#` heading.
    fn heading(line: &str) -> Option<(usize, &str)> {
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
    /// text in lowercase, keeping only word characters, spaces and dashes,
    /// with spaces made dashes.
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
}

/// kramdown parses Markdown inside an HTML element marked `markdown="1"`.
/// CommonMark only does so when blank lines separate the tags from the
/// Markdown, so this adds them.
struct MarkdownElements {
    out: String,
    /// The elements still open, innermost last.
    open: Vec<Element>,
    fence: Fence,
}

/// An open `markdown="1"` element.
struct Element {
    name: String,
    /// How many elements of the same name are open inside it.
    depth: usize,
}

impl MarkdownElements {
    fn separate(markdown: &str) -> Cow<'_, str> {
        if !markdown.contains("markdown=\"1\"") {
            return Cow::Borrowed(markdown);
        }
        let mut elements = Self {
            out: String::with_capacity(markdown.len() + 64),
            open: Vec::new(),
            fence: Fence::default(),
        };
        for line in markdown.split_inclusive('\n') {
            elements.line(line);
        }
        Cow::Owned(elements.out)
    }

    fn line(&mut self, line: &str) {
        if self.fence.contains(line) {
            self.out.push_str(line);
            return;
        }
        let trimmed = line.trim();
        if self.closes(trimmed) {
            self.out.push('\n');
            self.out.push_str(line);
            return;
        }
        self.out.push_str(line);
        if let Some(name) = Element::opened_by(trimmed) {
            if !line.ends_with('\n') {
                self.out.push('\n');
            }
            self.out.push('\n');
            self.open.push(Element { name, depth: 0 });
        }
    }

    /// Whether `trimmed` closes the innermost open element, keeping count of
    /// elements of the same name opened inside it.
    fn closes(&mut self, trimmed: &str) -> bool {
        let Some(element) = self.open.last_mut() else {
            return false;
        };
        if trimmed.starts_with(&format!("</{}", element.name)) {
            if element.depth == 0 {
                self.open.pop();
                return true;
            }
            element.depth -= 1;
        } else if trimmed.starts_with(&format!("<{}", element.name)) {
            element.depth += 1;
        }
        false
    }
}

impl Element {
    /// The name of the `markdown="1"` element `trimmed` opens, if it's a line
    /// of only that tag.
    fn opened_by(trimmed: &str) -> Option<String> {
        if !(trimmed.starts_with('<')
            && trimmed.ends_with('>')
            && trimmed.contains("markdown=\"1\""))
        {
            return None;
        }
        let name: String = trimmed[1..]
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric())
            .collect();
        (!name.is_empty()).then_some(name)
    }
}

/// Keeps track of fenced code blocks, where nothing is kramdown.
#[derive(Default)]
struct Fence {
    /// The marker of the open fence, if inside one.
    open: Option<&'static str>,
}

impl Fence {
    /// Whether `line` is part of a fenced code block, fences included.
    fn contains(&mut self, line: &str) -> bool {
        let trimmed = line.trim();
        if let Some(marker) = self.open {
            if trimmed.starts_with(marker) {
                self.open = None;
            }
            return true;
        }
        for marker in ["```", "~~~"] {
            if trimmed.starts_with(marker) {
                self.open = Some(marker);
                return true;
            }
        }
        false
    }
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
            Toc::expand(src),
            "# Title\n{:.no_toc}\n\n* [One `a`](#one-a)\n  * [Two](#two)\n* [One `a`](#one-a-1)\n\n## One `a`\n\n```\n# not\n```\n\n### Two\n\n## One `a`\n"
        );
        assert_eq!(
            Entry::header_id("4. Relating to Recent Posts"),
            "4-relating-to-recent-posts"
        );
        assert!(matches!(Toc::expand("* TOC\n"), Cow::Borrowed(_)));
    }

    #[test]
    fn span_classes_are_read() {
        assert_eq!(
            span_classes("{:.new}{: .old.x #id k=\"v\"}: rest"),
            ["new", "old", "x"]
        );
        assert!(span_classes("text {:.a}").is_empty());
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
