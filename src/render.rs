//! Walks pulldown-cmark's events and lays the document out in lines.

use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use ratatui_core::style::Style;

#[cfg(feature = "html")]
use crate::html::{self, Token};
use crate::table::Table;
use crate::wrap::{Inline, Piece};
use crate::writer::{Prefix, Writer};
use crate::{Heading, Rendered, Theme};

pub(crate) struct Renderer<'t> {
    theme: &'t Theme,
    writer: Writer,
    headings: Vec<Heading>,
    open: Vec<Open>,
    capture: Option<Capture>,
    table: Option<Table>,
}

impl<'t> Renderer<'t> {
    pub fn new(theme: &'t Theme, width: u16) -> Self {
        Self {
            theme,
            writer: Writer::new(width),
            headings: Vec::new(),
            open: Vec::new(),
            capture: None,
            table: None,
        }
    }

    pub fn run(mut self, markdown: &str) -> Rendered {
        #[cfg(feature = "kramdown")]
        let markdown = crate::kramdown::preprocess(markdown);
        #[cfg(not(feature = "kramdown"))]
        let markdown = std::borrow::Cow::Borrowed(markdown);
        for event in Parser::new_ext(&markdown, options()) {
            match self.capture.take() {
                Some(capture) => self.captured(capture, event),
                None => self.event(event),
            }
        }
        let (lines, links) = self.writer.finish();
        Rendered {
            lines,
            links,
            headings: self.headings,
            // Filled in by `render`, which splits it off first.
            #[cfg(feature = "front-matter")]
            front_matter: None,
        }
    }

    /// Handles an event while `capture` is being read.
    fn captured(&mut self, capture: Capture, event: Event<'_>) {
        match (capture, event) {
            (Capture::Code { lang, mut text }, Event::Text(more)) => {
                text.push_str(&more);
                self.capture = Some(Capture::Code { lang, text });
            }
            (Capture::Code { lang, text }, Event::End(TagEnd::CodeBlock)) => {
                self.code_block(&lang, &text)
            }
            (Capture::Image { url, mut alt }, Event::Text(more) | Event::Code(more)) => {
                alt.push_str(&more);
                self.capture = Some(Capture::Image { url, alt });
            }
            (Capture::Image { url, alt }, Event::End(TagEnd::Image)) => self.image(&url, &alt),
            (Capture::HtmlBlock(mut html), Event::Html(more)) => {
                html.push_str(&more);
                self.capture = Some(Capture::HtmlBlock(html));
            }
            (Capture::HtmlBlock(html), Event::End(TagEnd::HtmlBlock)) => self.html_block(&html),
            #[cfg(feature = "html")]
            (pre @ Capture::Pre(_), Event::Html(html) | Event::InlineHtml(html)) => {
                self.capture = Some(pre);
                self.html_tokens(&html);
            }
            #[cfg(feature = "html")]
            (Capture::Pre(mut pre), Event::Text(more)) => {
                pre.push_str(&more);
                self.capture = Some(Capture::Pre(pre));
            }
            (capture, _) => self.capture = Some(capture),
        }
    }

    fn event(&mut self, event: Event<'_>) {
        match event {
            Event::Start(tag) => self.start(tag),
            Event::End(tag) => self.end(tag),
            Event::Text(text) => {
                // kramdown attribute lists, like `{:.no_toc}`, mean nothing
                // here and would otherwise show up as text.
                #[cfg(feature = "kramdown")]
                let text = crate::kramdown::strip_attribute_lists(&text);
                if !text.is_empty() {
                    self.text(text.to_string());
                }
            }
            Event::Code(code) => {
                let style = self.style().patch(self.theme.code);
                self.piece(code.into_string(), style);
            }
            Event::InlineMath(math) | Event::DisplayMath(math) => {
                let style = self.style().patch(self.theme.math);
                self.piece(math.into_string(), style);
            }
            Event::Html(html) | Event::InlineHtml(html) => self.html_tokens(&html),
            Event::FootnoteReference(label) => {
                let style = self.style().patch(self.theme.footnote);
                self.piece(format!("[{label}]"), style);
            }
            Event::SoftBreak => self.text(" "),
            Event::HardBreak => self.writer.line_break(),
            Event::Rule => {
                self.writer.flush();
                let rule = "─".repeat(self.writer.available());
                self.writer.emit(vec![Piece::new(rule, self.theme.rule)]);
                self.writer.set_gap(true);
            }
            Event::TaskListMarker(done) => {
                let mark = if done { "[x] " } else { "[ ] " };
                self.piece(mark, self.theme.list_marker);
            }
        }
    }

    fn start(&mut self, tag: Tag<'_>) {
        match tag {
            Tag::Paragraph => self.writer.flush(),
            Tag::Heading { level, .. } => {
                self.writer.flush();
                let level = heading_index(level);
                self.heading_marker(level);
                self.open(Kind::Heading, self.theme.headings[level]);
            }
            Tag::BlockQuote(_) => {
                self.writer.flush();
                // The gap before a quote goes outside it, without its bar.
                self.writer.pay_gap();
                let bar = Piece::new("▎ ", self.theme.quote_marker);
                self.open_prefixed(Kind::Quote, Prefix::same(bar), self.theme.quote);
            }
            Tag::CodeBlock(kind) => {
                self.writer.flush();
                let lang = match kind {
                    CodeBlockKind::Fenced(info) => {
                        info.split([' ', ',', '{']).next().unwrap_or("").to_owned()
                    }
                    CodeBlockKind::Indented => String::new(),
                };
                self.capture = Some(Capture::Code {
                    lang,
                    text: String::new(),
                });
            }
            Tag::HtmlBlock => {
                self.writer.flush();
                self.capture = Some(Capture::HtmlBlock(String::new()));
            }
            Tag::List(start) => {
                self.writer.flush();
                self.open(Kind::List(start), Style::new());
            }
            Tag::Item => {
                self.writer.flush();
                let depth = self
                    .open
                    .iter()
                    .filter(|o| matches!(o.kind, Kind::List(_)))
                    .count()
                    .saturating_sub(1);
                let list = self
                    .open
                    .iter_mut()
                    .rev()
                    .find(|o| matches!(o.kind, Kind::List(_)));
                let marker = match list.map(|o| &mut o.kind) {
                    Some(Kind::List(Some(n))) => {
                        *n += 1;
                        format!("{}. ", *n - 1)
                    }
                    _ => BULLETS[depth % BULLETS.len()].to_owned(),
                };
                let marker = Piece::new(marker, self.theme.list_marker);
                let prefix = Prefix::hanging(marker, self.theme.text);
                self.open_prefixed(Kind::Item, prefix, Style::new());
            }
            Tag::FootnoteDefinition(label) => {
                self.writer.flush();
                let marker = Piece::new(format!("[{label}] "), self.theme.footnote);
                let prefix = Prefix::hanging(marker, self.theme.text);
                self.open_prefixed(Kind::Footnote, prefix, Style::new());
            }
            Tag::DefinitionList | Tag::DefinitionListTitle => self.writer.flush(),
            Tag::DefinitionListDefinition => {
                self.writer.flush();
                let indent = Prefix::same(Piece::new("  ", self.theme.text));
                self.open_prefixed(Kind::Definition, indent, Style::new());
            }
            Tag::Table(alignments) => {
                self.writer.flush();
                self.table = Some(Table::new(alignments));
            }
            Tag::TableHead => self.open(Kind::TableHead, self.theme.table_header),
            Tag::TableRow | Tag::TableCell => {}
            Tag::Emphasis => self.open(Kind::Style, self.theme.emphasis),
            Tag::Strong => self.open(Kind::Style, self.theme.strong),
            Tag::Strikethrough => self.open(Kind::Style, self.theme.strikethrough),
            Tag::Superscript | Tag::Subscript => self.open(Kind::Style, Style::new()),
            Tag::Link { dest_url, .. } => self.open_link(Kind::Link, &dest_url),
            Tag::Image { dest_url, .. } => {
                self.capture = Some(Capture::Image {
                    url: dest_url.into_string(),
                    alt: String::new(),
                })
            }
            // Never parsed, as front matter is split off beforehand.
            Tag::MetadataBlock(_) => {}
        }
    }

    fn end(&mut self, tag: TagEnd) {
        match tag {
            TagEnd::Paragraph | TagEnd::DefinitionList => self.writer.end_block(),
            TagEnd::Heading(level) => {
                self.close(|k| *k == Kind::Heading);
                // Skip the `#` marks, if there are any.
                let marks = usize::from(self.theme.heading_marker.is_some());
                let text: String = self
                    .writer
                    .inline()
                    .iter()
                    .skip(marks)
                    .filter_map(|i| match i {
                        Inline::Piece(p) => Some(p.text.as_str()),
                        Inline::Break => None,
                    })
                    .collect();
                self.headings.push(Heading {
                    level: heading_index(level) as u8 + 1,
                    text: text.trim().to_owned(),
                    line: self.writer.next_block_line(),
                });
                self.writer.end_block();
            }
            TagEnd::BlockQuote(_) => {
                self.writer.flush();
                self.close(|k| *k == Kind::Quote);
                self.writer.set_gap(true);
            }
            TagEnd::List(_) => {
                self.writer.flush();
                self.close(|k| matches!(k, Kind::List(_)));
                // A list nested in a tight list's item runs straight on to
                // the next item.
                let nested = self.is_open(|k| matches!(k, Kind::List(_)));
                self.writer.set_gap(!nested);
            }
            TagEnd::Item => {
                self.writer.flush();
                self.close(|k| *k == Kind::Item);
            }
            TagEnd::DefinitionListDefinition => {
                self.writer.flush();
                self.close(|k| *k == Kind::Definition);
            }
            TagEnd::FootnoteDefinition => {
                self.writer.flush();
                self.close(|k| *k == Kind::Footnote);
                self.writer.set_gap(true);
            }
            TagEnd::DefinitionListTitle => self.writer.flush(),
            TagEnd::TableCell => {
                let cell = self.writer.take_inline();
                if let Some(table) = &mut self.table {
                    table.push_cell(cell);
                }
            }
            TagEnd::TableHead => {
                self.close(|k| *k == Kind::TableHead);
                self.end_row();
            }
            TagEnd::TableRow => self.end_row(),
            TagEnd::Table => self.table(),
            TagEnd::Emphasis
            | TagEnd::Strong
            | TagEnd::Strikethrough
            | TagEnd::Superscript
            | TagEnd::Subscript => {
                self.close(|k| *k == Kind::Style);
            }
            TagEnd::Link => {
                self.close(|k| *k == Kind::Link);
            }
            // Read whole while captured, in `captured`.
            TagEnd::CodeBlock | TagEnd::Image | TagEnd::HtmlBlock | TagEnd::MetadataBlock(_) => {}
        }
    }

    fn code_block(&mut self, lang: &str, code: &str) {
        let code = code.replace('\t', "    ");
        let base = self.theme.code_block;
        let lines = highlighted(&code, lang, self.theme)
            .unwrap_or_else(|| code.lines().map(|l| vec![Piece::new(l, base)]).collect());
        let width = self.writer.available();
        // One column of padding on each side.
        let inner = width.saturating_sub(2).max(1);
        for line in lines {
            let mut rows = vec![Vec::new()];
            let mut used = 0;
            for piece in line {
                for c in piece.text.chars() {
                    let w = unicode_width::UnicodeWidthChar::width(c).unwrap_or(0);
                    if used + w > inner {
                        rows.push(Vec::new());
                        used = 0;
                    }
                    let row = rows.last_mut().expect("rows starts non-empty");
                    match row.last_mut() {
                        Some(Piece { text, style, .. }) if *style == piece.style => text.push(c),
                        _ => row.push(Piece::new(c.to_string(), piece.style)),
                    }
                    used += w;
                }
            }
            for row in rows {
                let used: usize = row.iter().map(Piece::width).sum();
                let mut pieces = vec![Piece::new(" ", base)];
                pieces.extend(row);
                pieces.push(Piece::new(" ".repeat(width.saturating_sub(used + 1)), base));
                self.writer.emit(pieces);
            }
        }
        self.writer.set_gap(true);
    }

    fn image(&mut self, url: &str, alt: &str) {
        let label = if alt.trim().is_empty() {
            "image".to_owned()
        } else {
            format!("image: {}", alt.trim())
        };
        let link = self.writer.link(url);
        let style = self.style().patch(self.theme.link).patch(self.theme.image);
        self.writer.piece(format!("[{label}]"), style, Some(link));
    }

    fn table(&mut self) {
        let Some(table) = self.table.take() else {
            return;
        };
        for line in table.lines(self.writer.available(), self.theme) {
            self.writer.emit(line);
        }
        self.writer.set_gap(true);
    }

    fn end_row(&mut self) {
        if let Some(table) = &mut self.table {
            table.end_row();
        }
    }

    /// Draws an HTML block once it's been read whole.
    fn html_block(&mut self, html: &str) {
        let before = self.writer.lines_written();
        self.html_tokens(html);
        #[cfg(feature = "html")]
        {
            // A `<pre>` left open ends with the block.
            if let Some(Capture::Pre(pre)) = self.capture.take() {
                self.pre(&pre);
            }
            // So do HTML styles and links.
            while matches!(
                self.open.last(),
                Some(Open {
                    kind: Kind::Html(_),
                    ..
                })
            ) {
                self.open.pop();
            }
        }
        // Lots of HTML, like a closing `</div>`, shows nothing, and
        // shouldn't leave a blank line behind.
        if self.writer.inline_is_blank() && self.writer.lines_written() == before {
            self.writer.take_inline();
        } else {
            self.writer.end_block();
        }
    }

    /// Without the `html` feature, HTML is left out.
    #[cfg(not(feature = "html"))]
    fn html_tokens(&mut self, _html: &str) {}

    /// Reads the text and links out of a chunk of HTML.
    #[cfg(feature = "html")]
    fn html_tokens(&mut self, html: &str) {
        for token in html::tokenize(html) {
            // Inside a `<pre>`, only its text and its end matter.
            if let Some(Capture::Pre(pre)) = &mut self.capture {
                match &token {
                    Token::Text(text) => pre.push_str(text),
                    Token::Close(name) if name == "pre" => {
                        if let Some(Capture::Pre(pre)) = self.capture.take() {
                            self.pre(&pre);
                        }
                    }
                    _ => {}
                }
                continue;
            }
            match &token {
                Token::Text(text) => self.text(text.clone()),
                Token::Open { name, .. } => match name.as_str() {
                    "a" => {
                        if let Some(href) = token.attr("href") {
                            self.open_link(Kind::Html(name.clone()), href);
                        }
                    }
                    "br" => self.writer.line_break(),
                    "pre" => {
                        self.writer.flush();
                        self.capture = Some(Capture::Pre(String::new()));
                    }
                    "li" => {
                        // Like a Markdown item, wrapped lines hang under
                        // the text, not the bullet.
                        self.writer.flush();
                        let bullet = Piece::new(BULLETS[0], self.theme.list_marker);
                        let prefix = Prefix::hanging(bullet, self.theme.text);
                        self.open_prefixed(Kind::HtmlItem, prefix, Style::new());
                    }
                    "p" | "div" | "ul" | "ol" | "tr" | "table" | "blockquote" | "section"
                    | "article" | "header" | "footer" | "figure" | "dl" | "dt" | "dd" => {
                        self.writer.hard_break()
                    }
                    "hr" => {
                        self.writer.hard_break();
                        self.text("───");
                        self.writer.line_break();
                    }
                    "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                        // A blank line before a heading, as in Markdown.
                        self.writer.hard_break();
                        if self.writer.inline().is_empty() {
                            self.writer.set_gap(true);
                        } else {
                            self.writer.line_break();
                        }
                        let level = usize::from(name.as_bytes()[1] - b'1');
                        self.heading_marker(level);
                        self.open(Kind::Html(name.clone()), self.theme.headings[level]);
                    }
                    "strong" | "b" => self.open(Kind::Html(name.clone()), self.theme.strong),
                    "em" | "i" | "cite" => self.open(Kind::Html(name.clone()), self.theme.emphasis),
                    "code" | "kbd" | "tt" | "samp" => {
                        self.open(Kind::Html(name.clone()), self.theme.code)
                    }
                    "s" | "del" | "strike" => {
                        self.open(Kind::Html(name.clone()), self.theme.strikethrough)
                    }
                    "small" | "sub" | "sup" => {
                        self.open(Kind::Html(name.clone()), self.theme.small)
                    }
                    "img" => {
                        let src = token.attr("src").unwrap_or("");
                        let alt = token.attr("alt").unwrap_or("");
                        self.image(src, alt);
                    }
                    "svg" => {
                        if let Some(label) = token.attr("aria-label") {
                            let style = self.style().patch(self.theme.image);
                            self.piece(format!("[figure: {label}]"), style);
                        }
                    }
                    _ => {}
                },
                Token::Close(name) => match name.as_str() {
                    "li" if self.is_open(|k| *k == Kind::HtmlItem) => {
                        self.writer.flush();
                        self.close(|k| *k == Kind::HtmlItem);
                    }
                    "p" | "div" | "li" | "ul" | "ol" | "tr" | "table" | "blockquote"
                    | "section" | "article" | "header" | "footer" | "figure" | "dl" | "dt"
                    | "dd" => self.writer.hard_break(),
                    "td" | "th" => self.text(" "),
                    _ => {
                        // Only when it's the innermost, as HTML is often
                        // left unclosed.
                        if matches!(self.open.last(), Some(o) if o.kind == Kind::Html(name.clone()))
                        {
                            self.open.pop();
                        }
                        if name.len() == 2 && name.starts_with('h') {
                            self.writer.hard_break();
                        }
                    }
                },
            }
        }
    }

    /// Draws the text of an HTML `<pre>` like a code block.
    #[cfg(feature = "html")]
    fn pre(&mut self, text: &str) {
        // Like a browser, skip one newline right after `<pre>`.
        let text = text.strip_prefix('\n').unwrap_or(text);
        self.code_block("", text);
    }

    /// The theme's `#` marks for a heading at `level`, counted from 0.
    fn heading_marker(&mut self, level: usize) {
        if let Some(style) = self.theme.heading_marker {
            self.piece(format!("{} ", "#".repeat(level + 1)), style);
        }
    }

    fn style(&self) -> Style {
        self.open.last().map_or(self.theme.text, |o| o.style)
    }

    fn link(&self) -> Option<usize> {
        self.open.iter().rev().find_map(|o| o.link)
    }

    fn piece(&mut self, text: impl Into<String>, style: Style) {
        let link = self.link();
        self.writer.piece(text, style, link);
    }

    fn text(&mut self, text: impl Into<String>) {
        let style = self.style();
        self.piece(text, style);
    }

    /// Opens an element whose text is styled with `patch`.
    fn open(&mut self, kind: Kind, patch: Style) {
        let style = self.style().patch(patch);
        self.open.push(Open {
            kind,
            style,
            link: None,
            prefixed: false,
        });
    }

    /// Opens an element that draws `prefix` before each of its lines.
    fn open_prefixed(&mut self, kind: Kind, prefix: Prefix, patch: Style) {
        self.writer.push_prefix(prefix);
        self.open(kind, patch);
        if let Some(open) = self.open.last_mut() {
            open.prefixed = true;
        }
    }

    fn open_link(&mut self, kind: Kind, url: &str) {
        let link = self.writer.link(url);
        self.open(kind, self.theme.link);
        if let Some(open) = self.open.last_mut() {
            open.link = Some(link);
        }
    }

    fn is_open(&self, kind: impl Fn(&Kind) -> bool) -> bool {
        self.open.iter().any(|o| kind(&o.kind))
    }

    /// Closes the innermost open element of `kind`, and anything left open
    /// inside it, like unclosed HTML.
    fn close(&mut self, kind: impl Fn(&Kind) -> bool) -> Option<Open> {
        if !self.is_open(&kind) {
            return None;
        }
        while let Some(open) = self.open.pop() {
            if open.prefixed {
                self.writer.pop_prefix();
            }
            if kind(&open.kind) {
                return Some(open);
            }
        }
        None
    }
}

/// An element that's open, and what it changed until it closes.
struct Open {
    kind: Kind,
    /// The style of text inside it.
    style: Style,
    /// The link text inside it points at.
    link: Option<usize>,
    /// Whether it pushed a prefix, to pop when it closes.
    prefixed: bool,
}

/// What an open element is, so its end closes the right one.
#[derive(PartialEq)]
enum Kind {
    /// Inline formatting, like emphasis.
    Style,
    Link,
    Heading,
    Quote,
    /// A list, counting its items when it's numbered.
    List(Option<u64>),
    Item,
    Footnote,
    Definition,
    TableHead,
    /// An HTML element that styles its text, or a link, by its tag name.
    #[cfg(feature = "html")]
    Html(String),
    #[cfg(feature = "html")]
    HtmlItem,
}

/// Content read whole before it's drawn, swallowing the events inside it.
enum Capture {
    Code {
        lang: String,
        text: String,
    },
    Image {
        url: String,
        alt: String,
    },
    HtmlBlock(String),
    /// The text of an HTML `<pre>`, kept as is until it closes.
    #[cfg(feature = "html")]
    Pre(String),
}

const BULLETS: [&str; 3] = ["• ", "◦ ", "▪ "];

fn options() -> Options {
    Options::ENABLE_TABLES
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_MATH
        | Options::ENABLE_SMART_PUNCTUATION
        | Options::ENABLE_HEADING_ATTRIBUTES
}

fn heading_index(level: HeadingLevel) -> usize {
    match level {
        HeadingLevel::H1 => 0,
        HeadingLevel::H2 => 1,
        HeadingLevel::H3 => 2,
        HeadingLevel::H4 => 3,
        HeadingLevel::H5 => 4,
        HeadingLevel::H6 => 5,
    }
}

#[cfg(feature = "syntax")]
fn highlighted(code: &str, lang: &str, theme: &Theme) -> Option<Vec<Vec<Piece>>> {
    let name = theme.syntax?;
    let lines = crate::highlight::highlight(code, lang, name)?;
    Some(
        lines
            .into_iter()
            .map(|line| {
                line.into_iter()
                    .map(|(text, color)| Piece::new(text, theme.code_block.fg(color)))
                    .collect()
            })
            .collect(),
    )
}

#[cfg(not(feature = "syntax"))]
fn highlighted(_: &str, _: &str, _: &Theme) -> Option<Vec<Vec<Piece>>> {
    None
}
