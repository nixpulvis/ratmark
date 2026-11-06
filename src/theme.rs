//! The styles a document is drawn with.

use ratatui_core::style::{Color, Modifier, Style};

/// The styles used for each kind of Markdown element.
///
/// Inline styles like [`Theme::strong`] are patched over the style of the
/// block they appear in, so a theme only needs to set what changes.
#[derive(Debug, Clone, PartialEq)]
pub struct Theme {
    /// Plain text, which everything else is patched over.
    pub text: Style,
    /// Heading text, by level from `#` to `######`.
    pub headings: [Style; 6],
    /// The style of `#` marks drawn in front of each heading, Markdown or
    /// HTML, to show its level. `None`, the default, draws no marks.
    pub heading_marker: Option<Style>,
    /// `*Emphasis*`.
    pub emphasis: Style,
    /// `**Strong**` text.
    pub strong: Style,
    /// `~~Struck through~~` text.
    pub strikethrough: Style,
    /// Link text.
    pub link: Style,
    /// An image's `[image: alt]` label, patched over the link style.
    pub image: Style,
    /// Inline `code`.
    pub code: Style,
    /// Fenced and indented code. The background fills the block's width.
    pub code_block: Style,
    /// Leave a column on either side of each code block, where `‹` and `›`
    /// show code cut off at the edges. Without it the code takes the full
    /// width, and the marks are drawn over its first and last columns.
    pub code_block_padding: bool,
    /// Text in a block quote.
    pub quote: Style,
    /// The bar down the side of a block quote.
    pub quote_marker: Style,
    /// List bullets and numbers, and task list boxes.
    pub list_marker: Style,
    /// A horizontal rule.
    pub rule: Style,
    /// Inline and display math, shown as written.
    pub math: Style,
    /// Footnote references and labels, like `[1]`.
    pub footnote: Style,
    /// The lines between a table's columns and rows.
    pub table_border: Style,
    /// A table's header row.
    pub table_header: Style,
    /// Draw a rule between body rows too, not just under the header.
    pub table_row_rules: bool,
    /// Text from `<small>` tags, used for things like dates under a title.
    pub small: Style,
    /// The syntect theme used to color code blocks, when the `syntax`
    /// feature is on. Only its foreground colors are used. Besides syntect's
    /// bundled themes there's `"ansi"`, which colors code with terminal
    /// color numbers, like [`Theme::ansi_dark`].
    ///
    /// As in [bat], a syntect color with an alpha of `00` names a terminal
    /// color by the number in its red, and an alpha of `01` keeps the code
    /// block's own foreground.
    ///
    /// [bat]: https://github.com/sharkdp/bat
    pub syntax: Option<&'static str>,
    /// Styles for elements with these classes, written like CSS class
    /// selectors, `.note` or `.note.warning`, and patched in order over
    /// everything inside a matching element. A code block inside takes its
    /// background. Classes come from HTML, with the `html` feature, and from
    /// a kramdown attribute list right after inline code, like
    /// `` `x`{:.new} ``, with the `kramdown` feature.
    pub classes: Vec<(String, Style)>,
}

impl Default for Theme {
    fn default() -> Self {
        Self::ansi_dark()
    }
}

impl Theme {
    /// The terminal's own colors, for a dark background.
    ///
    /// Every color is a terminal color number, never a fixed color, so the
    /// terminal's palette decides what each looks like, and text and the
    /// background are the terminal's own. Code, inline or in blocks, sits on
    /// gray 234 from the 256-color ramp, which suits a dark background.
    pub fn ansi_dark() -> Self {
        Self::ansi(234)
    }

    /// The terminal's own colors, for a light background. The same as
    /// [`Theme::ansi_dark`], but code sits on the light gray 255.
    pub fn ansi_light() -> Self {
        Self::ansi(255)
    }

    /// The terminal's own colors, with `shade` behind all code.
    fn ansi(shade: u8) -> Self {
        const GREEN: Color = Color::Indexed(2);
        const BLUE: Color = Color::Indexed(4);
        const MAGENTA: Color = Color::Indexed(5);
        const CYAN: Color = Color::Indexed(6);
        // Bright black, the terminal's muted gray.
        const GRAY: Color = Color::Indexed(8);

        let bold = Style::new().add_modifier(Modifier::BOLD);
        let gray = Style::new().fg(GRAY);
        Self {
            text: Style::new(),
            headings: [
                bold.add_modifier(Modifier::UNDERLINED),
                bold,
                bold,
                bold,
                bold,
                bold,
            ],
            heading_marker: None,
            emphasis: Style::new().add_modifier(Modifier::ITALIC),
            strong: bold,
            strikethrough: Style::new().add_modifier(Modifier::CROSSED_OUT),
            link: Style::new().fg(BLUE).add_modifier(Modifier::UNDERLINED),
            image: Style::new().fg(MAGENTA),
            code: Style::new().fg(GREEN).bg(Color::Indexed(shade)),
            code_block: Style::new().bg(Color::Indexed(shade)),
            code_block_padding: true,
            quote: Style::new().add_modifier(Modifier::ITALIC),
            quote_marker: gray,
            list_marker: Style::new(),
            rule: gray,
            math: Style::new().fg(CYAN),
            footnote: gray,
            table_border: gray,
            table_header: bold,
            table_row_rules: true,
            small: gray,
            syntax: Some("ansi"),
            classes: Vec::new(),
        }
    }

    /// No colors at all, only bold, italic and the like, so the terminal's own
    /// colors show through.
    pub fn plain() -> Self {
        let bold = Style::new().add_modifier(Modifier::BOLD);
        let dim = Style::new().add_modifier(Modifier::DIM);
        Self {
            text: Style::new(),
            headings: [
                bold.add_modifier(Modifier::UNDERLINED),
                bold,
                bold,
                bold,
                bold,
                bold,
            ],
            heading_marker: None,
            emphasis: Style::new().add_modifier(Modifier::ITALIC),
            strong: bold,
            strikethrough: Style::new().add_modifier(Modifier::CROSSED_OUT),
            link: Style::new().add_modifier(Modifier::UNDERLINED),
            image: Style::new().add_modifier(Modifier::UNDERLINED),
            code: Style::new().add_modifier(Modifier::REVERSED),
            code_block: Style::new(),
            code_block_padding: true,
            quote: Style::new().add_modifier(Modifier::ITALIC),
            quote_marker: dim,
            list_marker: Style::new(),
            rule: dim,
            math: Style::new().add_modifier(Modifier::ITALIC),
            footnote: Style::new(),
            table_border: dim,
            table_header: bold,
            table_row_rules: true,
            small: dim,
            syntax: None,
            classes: Vec::new(),
        }
    }

    /// The style for an element with `classes`, from [`Theme::classes`],
    /// or `None` when no selector matches.
    #[cfg(any(feature = "html", feature = "kramdown"))]
    pub(crate) fn class_style<'c>(
        &self,
        classes: impl IntoIterator<Item = &'c str> + Clone,
    ) -> Option<Style> {
        let has = |class: &str| classes.clone().into_iter().any(|c| c == class);
        self.classes
            .iter()
            .filter(|(selector, _)| {
                let mut wanted = selector.split('.').filter(|c| !c.is_empty()).peekable();
                wanted.peek().is_some() && wanted.all(has)
            })
            .map(|(_, style)| *style)
            .reduce(|style, more| style.patch(more))
    }
}
