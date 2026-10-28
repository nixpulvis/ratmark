//! Render Markdown into [Ratatui] text.
//!
//! [`render`] lays a document out for a given width and returns its lines,
//! ready for a `Paragraph`, along with where each link landed so a client can
//! make them clickable or let the keyboard move between them.
//!
//! ```
//! let rendered = ratmark::render("# Hi\n\nSee [the docs](https://docs.rs).", 40, &ratmark::Theme::default());
//! assert_eq!(rendered.links[0].url, "https://docs.rs");
//! ```
//!
//! CommonMark plus tables, footnotes, strikethrough, task lists, math and
//! front matter are supported. HTML embedded in the Markdown is read for its
//! text and links. A few kramdown habits are understood too, like attribute
//! lists and Markdown inside `markdown="1"` elements. Code blocks are syntax highlighted with the `highlight`
//! feature, which is on by default.
//!
//! [Ratatui]: https://ratatui.rs

use ratatui_core::text::{Line, Text};

#[cfg(feature = "highlight")]
mod highlight;
mod html;
mod kramdown;
mod render;
mod theme;
mod wrap;

pub use theme::Theme;

/// Lays `markdown` out in lines at most `width` columns wide.
pub fn render(markdown: &str, width: u16, theme: &Theme) -> Rendered {
    render::Renderer::new(theme, width).run(markdown)
}

/// A rendered document.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Rendered {
    /// The document's lines, already wrapped. Draw them without wrapping.
    pub lines: Vec<Line<'static>>,
    /// Every link, in the order they appear.
    pub links: Vec<Link>,
    /// The front matter between `---` or `+++` lines at the top, unparsed.
    pub front_matter: Option<String>,
    /// Every heading, in order, for jumping to a section or listing them.
    pub headings: Vec<Heading>,
}

impl Rendered {
    /// The lines as a [`Text`].
    pub fn text(&self) -> Text<'static> {
        Text::from(self.lines.clone())
    }

    /// The link drawn at column `x` of line `line`, if there is one.
    pub fn link_at(&self, line: usize, x: u16) -> Option<&Link> {
        self.links.iter().find(|link| {
            link.regions
                .iter()
                .any(|r| r.line == line && (r.start..r.end).contains(&x))
        })
    }
}

/// A heading and the line it starts on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Heading {
    /// 1 for `#` through 6 for `######`.
    pub level: u8,
    pub text: String,
    pub line: usize,
}

/// A link and the places it was drawn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
    pub url: String,
    /// Where the link's text landed. A link that wraps has one region per
    /// line.
    pub regions: Vec<Region>,
}

/// A stretch of one line, in columns from the start of the line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Region {
    pub line: usize,
    pub start: u16,
    pub end: u16,
}
