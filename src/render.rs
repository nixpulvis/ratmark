//! Walks pulldown-cmark's events and lays the document out in lines.

use pulldown_cmark::{Alignment, CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use ratatui_core::style::Style;
use ratatui_core::text::{Line, Span};
use unicode_width::UnicodeWidthStr;

use crate::html::{self, Token};
use crate::wrap::{Inline, Piece, wrap};
use crate::{Heading, Link, Region, Rendered, Theme};

pub(crate) fn options() -> Options {
    Options::ENABLE_TABLES
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_MATH
        | Options::ENABLE_SMART_PUNCTUATION
        | Options::ENABLE_HEADING_ATTRIBUTES
}

const BULLETS: [&str; 3] = ["• ", "◦ ", "▪ "];

/// Something drawn at the start of every line of a block, like a block
/// quote's bar or a list item's bullet. `first` is used on the block's first
/// line and `rest` after.
struct Prefix {
    first: Vec<Piece>,
    rest: Vec<Piece>,
    used: bool,
}

struct Table {
    alignments: Vec<Alignment>,
    rows: Vec<Vec<Vec<Inline>>>,
    row: Vec<Vec<Inline>>,
}

pub(crate) struct Renderer<'t> {
    theme: &'t Theme,
    width: usize,
    lines: Vec<Line<'static>>,
    links: Vec<Link>,
    headings: Vec<Heading>,

    inline: Vec<Inline>,
    styles: Vec<Style>,
    link_stack: Vec<usize>,
    prefixes: Vec<Prefix>,
    /// HTML `<li>`s open, each with a prefix to pop when it closes.
    html_items: usize,
    lists: Vec<Option<u64>>,
    /// Whether a blank line is owed before the next block.
    gap: bool,

    code: Option<(String, String)>,
    html: Option<String>,
    /// Open HTML elements that pushed a style, so closing them pops it.
    html_styles: Vec<String>,
    image: Option<(String, String)>,
    table: Option<Table>,
}

impl<'t> Renderer<'t> {
    pub fn new(theme: &'t Theme, width: u16) -> Self {
        Self {
            theme,
            width: usize::from(width).max(1),
            lines: Vec::new(),
            links: Vec::new(),
            headings: Vec::new(),
            inline: Vec::new(),
            styles: vec![theme.text],
            link_stack: Vec::new(),
            prefixes: Vec::new(),
            html_items: 0,
            lists: Vec::new(),
            gap: false,
            code: None,
            html: None,
            html_styles: Vec::new(),
            image: None,
            table: None,
        }
    }

    pub fn run(mut self, markdown: &str) -> Rendered {
        #[cfg(feature = "kramdown")]
        let markdown = crate::kramdown::preprocess(markdown);
        #[cfg(feature = "kramdown")]
        let markdown: &str = &markdown;
        for event in Parser::new_ext(markdown, options()) {
            self.event(event);
        }
        self.flush();
        Rendered {
            lines: self.lines,
            links: self.links,
            headings: self.headings,
            // Filled in by `render`, which splits it off first.
            #[cfg(feature = "front-matter")]
            front_matter: None,
        }
    }

    fn style(&self) -> Style {
        *self.styles.last().expect("base style is never popped")
    }

    fn push_style(&mut self, patch: Style) {
        self.styles.push(self.style().patch(patch));
    }

    fn pop_style(&mut self) {
        if self.styles.len() > 1 {
            self.styles.pop();
        }
    }

    fn piece(&mut self, text: impl Into<String>, style: Style) {
        self.inline.push(Inline::Piece(Piece {
            text: text.into(),
            style,
            link: self.link_stack.last().copied(),
        }));
    }

    fn text(&mut self, text: impl Into<String>) {
        let style = self.style();
        self.piece(text, style);
    }

    /// Ends the current line, unless nothing but whitespace is on it.
    fn hard_break(&mut self) {
        let blank = self
            .inline
            .iter()
            .rev()
            .take_while(|i| matches!(i, Inline::Piece(_)))
            .all(|i| matches!(i, Inline::Piece(p) if p.text.trim().is_empty()));
        if !blank {
            self.inline.push(Inline::Break);
        }
    }

    fn start_link(&mut self, url: &str) {
        self.links.push(Link {
            url: url.to_owned(),
            regions: Vec::new(),
        });
        self.link_stack.push(self.links.len() - 1);
        self.push_style(self.theme.link);
    }

    fn end_link(&mut self) {
        self.link_stack.pop();
        self.pop_style();
    }

    fn prefix_width(&self) -> usize {
        self.prefixes
            .iter()
            .map(|p| {
                let pieces = if p.used { &p.rest } else { &p.first };
                pieces.iter().map(Piece::width).sum::<usize>()
            })
            .sum()
    }

    /// Room left for content once the prefixes are drawn.
    fn available(&self) -> usize {
        self.width.saturating_sub(self.prefix_width()).max(1)
    }

    /// Writes one line of pieces after the current prefixes, recording where
    /// its links landed. Anything past the width, which only deeply nested
    /// prefixes in a very narrow width can cause, is cut off.
    fn emit(&mut self, pieces: Vec<Piece>) {
        if std::mem::take(&mut self.gap) && !self.lines.is_empty() {
            self.blank();
        }
        let mut all = Vec::new();
        for prefix in &mut self.prefixes {
            all.extend(
                if prefix.used {
                    &prefix.rest
                } else {
                    &prefix.first
                }
                .iter()
                .cloned(),
            );
            prefix.used = true;
        }
        all.extend(pieces);
        truncate(&mut all, self.width);

        let line = self.lines.len();
        let mut x = 0;
        for piece in &all {
            let w = piece.width() as u16;
            if let Some(id) = piece.link {
                let regions = &mut self.links[id].regions;
                match regions.last_mut() {
                    Some(r) if r.line == line && r.end == x => r.end += w,
                    _ => regions.push(Region {
                        line,
                        start: x,
                        end: x + w,
                    }),
                }
            }
            x += w;
        }
        let spans: Vec<Span<'static>> = all
            .into_iter()
            .map(|p| Span::styled(p.text, p.style))
            .collect();
        self.lines.push(Line::from(spans));
    }

    /// Writes a blank line, keeping the bars of any block quotes.
    fn blank(&mut self) {
        let mut pieces: Vec<Piece> = self
            .prefixes
            .iter()
            .flat_map(|p| p.rest.iter().cloned())
            .collect();
        truncate(&mut pieces, self.width);
        if let Some(last) = pieces.last_mut() {
            last.text.truncate(last.text.trim_end().len());
        }
        let spans: Vec<Span<'static>> = pieces
            .into_iter()
            .filter(|p| !p.text.is_empty())
            .map(|p| Span::styled(p.text, p.style))
            .collect();
        self.lines.push(Line::from(spans));
    }

    /// Lays out any pending inline content.
    fn flush(&mut self) {
        while matches!(self.inline.last(), Some(Inline::Break)) {
            self.inline.pop();
        }
        if self.inline.is_empty() {
            return;
        }
        // Every prefix is as wide on its first line as on the rest, so one
        // width holds for the whole block.
        let inline = std::mem::take(&mut self.inline);
        for line in wrap(&inline, self.available()) {
            self.emit(line);
        }
    }

    /// Starts a block. Any blank line owed before it is written with its
    /// first line, so a block that turns out empty leaves no gap.
    fn begin_block(&mut self) {
        self.flush();
    }

    fn end_block(&mut self) {
        self.flush();
        self.gap = true;
    }

    fn event(&mut self, event: Event<'_>) {
        if let Some((_, code)) = &mut self.code {
            match event {
                Event::Text(text) => code.push_str(&text),
                Event::End(TagEnd::CodeBlock) => self.code_block(),
                _ => {}
            }
            return;
        }
        if let Some((_, alt)) = &mut self.image {
            match event {
                Event::Text(text) | Event::Code(text) => alt.push_str(&text),
                Event::End(TagEnd::Image) => self.image(),
                _ => {}
            }
            return;
        }

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
            Event::Html(html) => {
                if let Some(buffer) = &mut self.html {
                    buffer.push_str(&html);
                } else {
                    self.html_tokens(&html);
                }
            }
            Event::InlineHtml(html) => self.html_tokens(&html),
            Event::FootnoteReference(label) => {
                let style = self.style().patch(self.theme.footnote);
                self.piece(format!("[{label}]"), style);
            }
            Event::SoftBreak => self.text(" "),
            Event::HardBreak => self.inline.push(Inline::Break),
            Event::Rule => {
                self.begin_block();
                let rule = "─".repeat(self.available());
                self.emit(vec![Piece::new(rule, self.theme.rule)]);
                self.gap = true;
            }
            Event::TaskListMarker(done) => {
                let mark = if done { "[x] " } else { "[ ] " };
                self.piece(mark, self.theme.list_marker);
            }
        }
    }

    fn start(&mut self, tag: Tag<'_>) {
        match tag {
            Tag::Paragraph => self.begin_block(),
            Tag::Heading { level, .. } => {
                self.begin_block();
                let level = heading_index(level);
                self.heading_marker(level);
                self.push_style(self.theme.headings[level]);
            }
            Tag::BlockQuote(_) => {
                self.begin_block();
                // The gap before a quote goes outside it, without its bar.
                if std::mem::take(&mut self.gap) && !self.lines.is_empty() {
                    self.blank();
                }
                let bar = Piece::new("▎ ", self.theme.quote_marker);
                self.prefixes.push(Prefix {
                    first: vec![bar.clone()],
                    rest: vec![bar],
                    used: false,
                });
                self.push_style(self.theme.quote);
            }
            Tag::CodeBlock(kind) => {
                self.begin_block();
                let lang = match kind {
                    CodeBlockKind::Fenced(info) => {
                        info.split([' ', ',', '{']).next().unwrap_or("").to_owned()
                    }
                    CodeBlockKind::Indented => String::new(),
                };
                self.code = Some((lang, String::new()));
            }
            Tag::HtmlBlock => {
                self.flush();
                self.html = Some(String::new());
            }
            Tag::List(start) => {
                self.begin_block();
                self.lists.push(start);
            }
            Tag::Item => {
                self.flush();
                let depth = self.lists.len().saturating_sub(1);
                let marker = match self.lists.last_mut() {
                    Some(Some(n)) => {
                        *n += 1;
                        format!("{}. ", *n - 1)
                    }
                    _ => BULLETS[depth % BULLETS.len()].to_owned(),
                };
                let indent = " ".repeat(marker.width());
                self.prefixes.push(Prefix {
                    first: vec![Piece::new(marker, self.theme.list_marker)],
                    rest: vec![Piece::new(indent, self.theme.text)],
                    used: false,
                });
            }
            Tag::FootnoteDefinition(label) => {
                self.begin_block();
                let marker = format!("[{label}] ");
                let indent = " ".repeat(marker.width());
                self.prefixes.push(Prefix {
                    first: vec![Piece::new(marker, self.theme.footnote)],
                    rest: vec![Piece::new(indent, self.theme.text)],
                    used: false,
                });
            }
            Tag::DefinitionList => self.begin_block(),
            Tag::DefinitionListTitle => self.flush(),
            Tag::DefinitionListDefinition => {
                self.flush();
                self.prefixes.push(Prefix {
                    first: vec![Piece::new("  ", self.theme.text)],
                    rest: vec![Piece::new("  ", self.theme.text)],
                    used: false,
                });
            }
            Tag::Table(alignments) => {
                self.begin_block();
                self.table = Some(Table {
                    alignments,
                    rows: Vec::new(),
                    row: Vec::new(),
                });
            }
            Tag::TableHead => self.push_style(self.theme.table_header),
            Tag::TableRow | Tag::TableCell => {}
            Tag::Emphasis => self.push_style(self.theme.emphasis),
            Tag::Strong => self.push_style(self.theme.strong),
            Tag::Strikethrough => self.push_style(self.theme.strikethrough),
            Tag::Superscript | Tag::Subscript => self.push_style(Style::new()),
            Tag::Link { dest_url, .. } => self.start_link(&dest_url),
            Tag::Image { dest_url, .. } => {
                self.image = Some((dest_url.into_string(), String::new()))
            }
            // Never parsed, as front matter is split off beforehand.
            Tag::MetadataBlock(_) => {}
        }
    }

    fn end(&mut self, tag: TagEnd) {
        match tag {
            TagEnd::Paragraph => self.end_block(),
            TagEnd::Heading(level) => {
                self.pop_style();
                // Skip the `#` marks, if there are any.
                let marks = usize::from(self.theme.heading_marker.is_some());
                let text: String = self
                    .inline
                    .iter()
                    .skip(marks)
                    .filter_map(|i| match i {
                        Inline::Piece(p) => Some(p.text.as_str()),
                        Inline::Break => None,
                    })
                    .collect();
                let line = self.lines.len() + usize::from(self.gap && !self.lines.is_empty());
                self.headings.push(Heading {
                    level: heading_index(level) as u8 + 1,
                    text: text.trim().to_owned(),
                    line,
                });
                self.end_block();
            }
            TagEnd::BlockQuote(_) => {
                self.flush();
                self.prefixes.pop();
                self.pop_style();
                self.gap = true;
            }
            TagEnd::HtmlBlock => {
                let html = self.html.take().unwrap_or_default();
                let before = self.lines.len();
                self.html_tokens(&html);
                // Styles left open by unbalanced HTML stop with the block.
                for name in std::mem::take(&mut self.html_styles) {
                    if name == "a" {
                        self.end_link();
                    } else {
                        self.pop_style();
                    }
                }
                // Lots of HTML, like a closing `</div>`, shows nothing, and
                // shouldn't leave a blank line behind.
                let empty = self.inline.iter().all(|i| match i {
                    Inline::Piece(p) => p.text.trim().is_empty(),
                    Inline::Break => true,
                });
                if empty && self.lines.len() == before {
                    self.inline.clear();
                } else {
                    self.end_block();
                }
            }
            TagEnd::List(_) => {
                self.flush();
                self.lists.pop();
                // A list nested in a tight list's item runs straight on to
                // the next item.
                self.gap = self.lists.is_empty();
            }
            TagEnd::Item | TagEnd::DefinitionListDefinition => {
                self.flush();
                self.prefixes.pop();
            }
            TagEnd::FootnoteDefinition => {
                self.flush();
                self.prefixes.pop();
                self.gap = true;
            }
            TagEnd::DefinitionList => self.end_block(),
            TagEnd::DefinitionListTitle => self.flush(),
            TagEnd::TableCell => {
                let cell = std::mem::take(&mut self.inline);
                if let Some(table) = &mut self.table {
                    table.row.push(cell);
                }
            }
            TagEnd::TableHead => {
                self.pop_style();
                self.end_row();
            }
            TagEnd::TableRow => self.end_row(),
            TagEnd::Table => self.table(),
            TagEnd::Emphasis
            | TagEnd::Strong
            | TagEnd::Strikethrough
            | TagEnd::Superscript
            | TagEnd::Subscript => self.pop_style(),
            TagEnd::Link => self.end_link(),
            // Handled while capturing their content, in `event`.
            TagEnd::CodeBlock | TagEnd::Image | TagEnd::MetadataBlock(_) => {}
        }
    }

    fn image(&mut self) {
        let Some((url, alt)) = self.image.take() else {
            return;
        };
        let label = if alt.trim().is_empty() {
            "image".to_owned()
        } else {
            format!("image: {}", alt.trim())
        };
        self.start_link(&url);
        let style = self.style().patch(self.theme.image);
        self.piece(format!("[{label}]"), style);
        self.end_link();
    }

    fn code_block(&mut self) {
        let Some((lang, code)) = self.code.take() else {
            return;
        };
        let code = code.replace('\t', "    ");
        let base = self.theme.code_block;
        let lines = highlighted(&code, &lang, self.theme)
            .unwrap_or_else(|| code.lines().map(|l| vec![Piece::new(l, base)]).collect());
        let width = self.available();
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
                self.emit(pieces);
            }
        }
        self.gap = true;
    }

    fn end_row(&mut self) {
        if let Some(table) = &mut self.table {
            let row = std::mem::take(&mut table.row);
            table.rows.push(row);
        }
    }

    fn table(&mut self) {
        let Some(table) = self.table.take() else {
            return;
        };
        let columns = table.rows.iter().map(Vec::len).max().unwrap_or(0);
        if columns == 0 {
            return;
        }
        let cell_width = |cell: &Vec<Inline>| {
            wrap(cell, usize::MAX / 2)
                .iter()
                .map(|l| l.iter().map(Piece::width).sum::<usize>())
                .max()
                .unwrap_or(0)
        };
        let mut widths = vec![1; columns];
        for row in &table.rows {
            for (i, cell) in row.iter().enumerate() {
                widths[i] = widths[i].max(cell_width(cell));
            }
        }
        // Narrow the widest column until the table fits, wrapping its cells.
        let available = self.available();
        let separators = 3 * (columns - 1);
        while widths.iter().sum::<usize>() + separators > available {
            let (i, &widest) = widths
                .iter()
                .enumerate()
                .max_by_key(|(_, w)| **w)
                .expect("columns > 0");
            if widest <= 3 {
                break;
            }
            widths[i] -= 1;
        }

        let border = self.theme.table_border;
        for (r, row) in table.rows.iter().enumerate() {
            let cells: Vec<Vec<Vec<Piece>>> = (0..columns)
                .map(|i| {
                    row.get(i)
                        .map(|cell| wrap(cell, widths[i]))
                        .unwrap_or_default()
                })
                .collect();
            let height = cells.iter().map(Vec::len).max().unwrap_or(0).max(1);
            for y in 0..height {
                let mut pieces = Vec::new();
                for (i, cell) in cells.iter().enumerate() {
                    if i > 0 {
                        pieces.push(Piece::new(" │ ", border));
                    }
                    let line = cell.get(y).cloned().unwrap_or_default();
                    let used: usize = line.iter().map(Piece::width).sum();
                    let pad = widths[i].saturating_sub(used);
                    let (left, right) = match table.alignments.get(i) {
                        Some(Alignment::Right) => (pad, 0),
                        Some(Alignment::Center) => (pad / 2, pad - pad / 2),
                        _ => (0, pad),
                    };
                    pieces.push(Piece::new(" ".repeat(left), self.theme.text));
                    pieces.extend(line);
                    pieces.push(Piece::new(" ".repeat(right), self.theme.text));
                }
                self.emit(pieces);
            }
            let last = r + 1 == table.rows.len();
            if !last && (r == 0 || self.theme.table_row_rules) {
                let rule = widths
                    .iter()
                    .map(|w| "─".repeat(*w))
                    .collect::<Vec<_>>()
                    .join("─┼─");
                self.emit(vec![Piece::new(rule, border)]);
            }
        }
        self.gap = true;
    }

    /// Reads the text and links out of a chunk of HTML.
    fn html_tokens(&mut self, html: &str) {
        for token in html::tokenize(html) {
            match &token {
                Token::Text(text) => self.text(text.clone()),
                Token::Open { name, .. } => match name.as_str() {
                    "a" => {
                        if let Some(href) = token.attr("href") {
                            self.start_link(href);
                            self.html_styles.push("a".into());
                        }
                    }
                    "br" => self.inline.push(Inline::Break),
                    "li" => {
                        // Like a Markdown item, wrapped lines hang under
                        // the text, not the bullet.
                        self.flush();
                        let indent = " ".repeat(BULLETS[0].width());
                        self.prefixes.push(Prefix {
                            first: vec![Piece::new(BULLETS[0], self.theme.list_marker)],
                            rest: vec![Piece::new(indent, self.theme.text)],
                            used: false,
                        });
                        self.html_items += 1;
                    }
                    "p" | "div" | "ul" | "ol" | "tr" | "table" | "blockquote" | "section"
                    | "article" | "header" | "footer" | "figure" | "dl" | "dt" | "dd" => {
                        self.hard_break()
                    }
                    "hr" => {
                        self.hard_break();
                        self.text("───");
                        self.inline.push(Inline::Break);
                    }
                    "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                        // A blank line before a heading, as in Markdown.
                        self.hard_break();
                        if self.inline.is_empty() {
                            self.gap = true;
                        } else {
                            self.inline.push(Inline::Break);
                        }
                        let level = usize::from(name.as_bytes()[1] - b'1');
                        self.heading_marker(level);
                        self.styled_html(name, self.theme.headings[level]);
                    }
                    "strong" | "b" => self.styled_html(name, self.theme.strong),
                    "em" | "i" | "cite" => self.styled_html(name, self.theme.emphasis),
                    "code" | "kbd" | "tt" | "samp" => self.styled_html(name, self.theme.code),
                    "s" | "del" | "strike" => self.styled_html(name, self.theme.strikethrough),
                    "small" | "sub" | "sup" => self.styled_html(name, self.theme.small),
                    "img" => {
                        let src = token.attr("src").unwrap_or("").to_owned();
                        let alt = token.attr("alt").unwrap_or("").to_owned();
                        self.image = Some((src, alt));
                        self.image();
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
                    "li" if self.html_items > 0 => {
                        self.flush();
                        self.prefixes.pop();
                        self.html_items -= 1;
                    }
                    "p" | "div" | "li" | "ul" | "ol" | "tr" | "table" | "blockquote"
                    | "section" | "article" | "header" | "footer" | "figure" | "dl" | "dt"
                    | "dd" => self.hard_break(),
                    "td" | "th" => self.text(" "),
                    _ => {
                        if self.html_styles.last() == Some(name) {
                            self.html_styles.pop();
                            if name == "a" {
                                self.end_link();
                            } else {
                                self.pop_style();
                            }
                        }
                        if name.len() == 2 && name.starts_with('h') {
                            self.hard_break();
                        }
                    }
                },
            }
        }
    }

    /// The theme's `#` marks for a heading at `level`, counted from 0.
    fn heading_marker(&mut self, level: usize) {
        if let Some(style) = self.theme.heading_marker {
            self.piece(format!("{} ", "#".repeat(level + 1)), style);
        }
    }

    fn styled_html(&mut self, name: &str, style: Style) {
        self.html_styles.push(name.to_owned());
        self.push_style(style);
    }
}

/// Cuts pieces off at `width` columns.
fn truncate(pieces: &mut Vec<Piece>, width: usize) {
    let mut used = 0;
    for i in 0..pieces.len() {
        let w = pieces[i].width();
        if used + w <= width {
            used += w;
            continue;
        }
        let mut text = String::new();
        for c in pieces[i].text.chars() {
            let cw = unicode_width::UnicodeWidthChar::width(c).unwrap_or(0);
            if used + cw > width {
                break;
            }
            used += cw;
            text.push(c);
        }
        pieces[i].text = text;
        pieces.truncate(i + 1);
        break;
    }
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

#[cfg(feature = "highlight")]
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

#[cfg(not(feature = "highlight"))]
fn highlighted(_: &str, _: &str, _: &Theme) -> Option<Vec<Vec<Piece>>> {
    None
}
