//! Small allowances for Markdown written for kramdown, Jekyll's default.
//!
//! [`Kramdown::to_commonmark`] rewrites what a CommonMark parser would read
//! differently, and [`AttributeList`] reads the `{:.class}` lists it leaves
//! in the text. They're what this crate renders kramdown with, and they're
//! public for anyone turning the same Markdown into something else, like
//! HTML.
//!
//! ```
//! use ratmark::kramdown::{AttributeList, Kramdown};
//!
//! let markdown = Kramdown::to_commonmark("* TOC\n{:toc}\n\n## Hi there\n");
//! assert_eq!(markdown, "* [Hi there](#hi-there)\n\n## Hi there\n");
//!
//! let (list, rest) = AttributeList::split("{:.new #a}: rest");
//! assert_eq!(list.classes, ["new"]);
//! assert_eq!(list.id, Some("a"));
//! assert_eq!(rest, ": rest");
//! ```

use std::borrow::Cow;

/// Markdown written for kramdown.
pub struct Kramdown;

impl Kramdown {
    /// Rewrites what CommonMark would read differently from kramdown.
    /// `{:toc}` lists become lists of links to the headings, Markdown inside
    /// `markdown="1"` elements gets the blank lines CommonMark needs to read
    /// it as Markdown, and a line of only a span tag, like `<img>`, starts a
    /// paragraph rather than an HTML block. Attribute lists are left in
    /// place.
    pub fn to_commonmark(markdown: &str) -> Cow<'_, str> {
        let markdown = Toc::expand(markdown);
        let markdown = match MarkdownElements::separate(&markdown) {
            Cow::Owned(out) => Cow::Owned(out),
            Cow::Borrowed(_) => markdown,
        };
        match SpanTags::join(&markdown) {
            Cow::Owned(out) => Cow::Owned(out),
            Cow::Borrowed(_) => markdown,
        }
    }

    /// The id kramdown gives a heading with GFM input, Jekyll's default: the
    /// text in lowercase, keeping only word characters, spaces and dashes,
    /// with spaces made dashes. See [`HeaderIds`] for ids unique to a
    /// document.
    pub fn header_id(text: &str) -> String {
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

/// Hands out heading ids the way kramdown does, a number added to any id
/// already taken in the document, like `intro-1`.
#[derive(Debug, Default)]
pub struct HeaderIds {
    taken: Vec<String>,
}

impl HeaderIds {
    /// The id for the next heading, with text `text`.
    pub fn next(&mut self, text: &str) -> String {
        let base = Kramdown::header_id(text);
        let mut id = base.clone();
        let mut n = 0;
        while self.taken.contains(&id) {
            n += 1;
            id = format!("{base}-{n}");
        }
        self.taken.push(id.clone());
        id
    }
}

/// The classes and id from kramdown attribute lists, like `{: .new #intro}`.
/// They come alone on a line after a block, or right after a span like
/// `` `code` ``, which puts them at the start of the following text.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AttributeList<'a> {
    pub id: Option<&'a str>,
    pub classes: Vec<&'a str>,
}

impl<'a> AttributeList<'a> {
    /// Splits the attribute lists at the start of `text` off the rest, read
    /// together as one. Other attributes, like `k="v"`, are skipped.
    pub fn split(text: &'a str) -> (Self, &'a str) {
        let mut list = Self::default();
        let mut text = text;
        while let Some(rest) = text.strip_prefix("{:") {
            let Some(end) = rest.find('}') else { break };
            for word in rest[..end].split_whitespace() {
                if let Some(id) = word.strip_prefix('#') {
                    list.id = Some(id);
                } else if word.starts_with('.') {
                    list.classes
                        .extend(word.split('.').filter(|c| !c.is_empty()));
                }
            }
            text = &rest[end + 1..];
        }
        (list, text)
    }

    /// Whether there's nothing in the list.
    pub fn is_empty(&self) -> bool {
        self.id.is_none() && self.classes.is_empty()
    }
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
        let mut ids = HeaderIds::default();
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
            let id = ids.next(text);
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

/// kramdown reads a line of only a span-level tag, like `<img>` or `<span>`,
/// as the start of a paragraph, Markdown on the lines after it included.
/// CommonMark starts an HTML block there instead, which runs to the next
/// blank line and leaves any Markdown in it as text. Joining the next line
/// onto the tag's makes the same paragraph of it.
struct SpanTags;

impl SpanTags {
    /// The span-level elements kramdown keeps in paragraphs.
    const NAMES: [&str; 38] = [
        "a", "abbr", "acronym", "b", "bdo", "big", "br", "button", "cite", "code", "del", "dfn",
        "em", "i", "img", "input", "ins", "kbd", "label", "mark", "option", "q", "rb", "rbc", "rp",
        "rt", "rtc", "ruby", "samp", "select", "small", "span", "strong", "sub", "sup", "textarea",
        "tt", "var",
    ];

    fn join(markdown: &str) -> Cow<'_, str> {
        let mut out = String::new();
        let mut joined = false;
        let mut fence = Fence::default();
        let mut lines = markdown.split_inclusive('\n').peekable();
        while let Some(line) = lines.next() {
            if fence.contains(line) {
                out.push_str(line);
                continue;
            }
            let next_is_text = lines.peek().is_some_and(|next| {
                let next = next.trim();
                !next.is_empty() && !next.starts_with("```") && !next.starts_with("~~~")
            });
            if next_is_text && Self::alone(line) {
                out.push_str(line.trim_end());
                out.push(' ');
                joined = true;
            } else {
                out.push_str(line);
            }
        }
        if joined {
            Cow::Owned(out)
        } else {
            Cow::Borrowed(markdown)
        }
    }

    /// Whether `line` is only a span-level tag, opening or closing, and not
    /// indented enough to be code.
    fn alone(line: &str) -> bool {
        let indent = line.len() - line.trim_start().len();
        let Some(tag) = line
            .trim()
            .strip_prefix('<')
            .and_then(|tag| tag.strip_suffix('>'))
            .filter(|tag| indent < 4 && !tag.contains(['<', '>']))
        else {
            return false;
        };
        let name: String = tag
            .trim_start_matches('/')
            .chars()
            .take_while(char::is_ascii_alphanumeric)
            .map(|c| c.to_ascii_lowercase())
            .collect();
        Self::NAMES.contains(&name.as_str())
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
            Kramdown::to_commonmark(src),
            "<div markdown=\"1\">\n\n```rust\n</div>\n```\n<div>\n</div>\n\n</div>\nafter\n"
        );
    }

    #[test]
    fn span_tags_start_paragraphs() {
        assert_eq!(
            Kramdown::to_commonmark("<img src=\"a\" />\n![b](c)\n"),
            "<img src=\"a\" /> ![b](c)\n"
        );
        assert_eq!(
            Kramdown::to_commonmark("<span class=\"g\">\n![a](b)\n![c](d)\n</span>\n"),
            "<span class=\"g\"> ![a](b)\n![c](d)\n</span>\n"
        );
        // Not block elements, tags before a blank line or a fence, or code.
        let src = "<div>\n*a*\n</div>\n\n<img />\n\n<b>\n```\n<b>\nx\n```\n    <b>\n    x\n";
        assert!(matches!(Kramdown::to_commonmark(src), Cow::Borrowed(_)));
    }

    #[test]
    fn tables_of_contents() {
        let src = "# Title\n{:.no_toc}\n\n* TOC\n{:toc}\n\n## One `a`\n\n```\n# not\n```\n\n### Two\n\n## One `a`\n";
        assert_eq!(
            Toc::expand(src),
            "# Title\n{:.no_toc}\n\n* [One `a`](#one-a)\n  * [Two](#two)\n* [One `a`](#one-a-1)\n\n## One `a`\n\n```\n# not\n```\n\n### Two\n\n## One `a`\n"
        );
        assert_eq!(
            Kramdown::header_id("4. Relating to Recent Posts"),
            "4-relating-to-recent-posts"
        );
        assert!(matches!(Toc::expand("* TOC\n"), Cow::Borrowed(_)));
    }

    #[test]
    fn attribute_lists() {
        let (list, rest) = AttributeList::split("{:.new}{: .old.x #id k=\"v\"}: rest");
        assert_eq!(list.classes, ["new", "old", "x"]);
        assert_eq!(list.id, Some("id"));
        assert_eq!(rest, ": rest");
        assert_eq!(AttributeList::split("{:.no_toc}").1, "");
        let (list, rest) = AttributeList::split("text {:.a}");
        assert!(list.is_empty());
        assert_eq!(rest, "text {:.a}");
        assert_eq!(AttributeList::split("{:unclosed").1, "{:unclosed");
    }

    #[test]
    fn header_ids_are_unique() {
        let mut ids = HeaderIds::default();
        assert_eq!(ids.next("One"), "one");
        assert_eq!(ids.next("One"), "one-1");
        assert_eq!(ids.next("one!"), "one-2");
    }

    #[test]
    fn leaves_other_documents_alone() {
        assert!(matches!(
            Kramdown::to_commonmark("<div>\n*hi*\n</div>"),
            Cow::Borrowed(_)
        ));
    }
}
