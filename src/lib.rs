//! Render Markdown into [Ratatui] text.
//!
//! [`MarkdownView`] is a widget that shows a document, scrolled, with links
//! that can be focused from the keyboard or found under a click. Its
//! [`MarkdownState`] holds the document and lays it out again whenever the
//! width changes.
//!
//! Underneath, [`render`] lays a document out for a given width and returns
//! its lines along with where each link landed, for drawing it some other
//! way.
//!
//! ```
//! let rendered = ratmark::render("# Hi\n\nSee [the docs](https://docs.rs).", 40, &ratmark::Theme::default());
//! assert_eq!(rendered.links[0].url, "https://docs.rs");
//! ```
//!
//! CommonMark plus tables, footnotes, strikethrough, task lists and math are
//! supported. With the `html` feature, HTML embedded in the Markdown is read
//! for its text, links and simple formatting, and without it HTML is left
//! out. Code blocks are syntax highlighted with the `syntax`
//! feature, which is off by default.
//!
//! The `front-matter` feature, off by default, splits YAML or TOML front
//! matter off the top of a document instead of rendering it. See
//! [`front_matter`].
//!
//! The `kramdown` feature, also off by default, understands a few habits of
//! Markdown written for kramdown, Jekyll's default: attribute lists like
//! `{:.no_toc}` are dropped, `{:toc}` lists become tables of contents, and
//! Markdown inside `markdown="1"` elements is rendered as Markdown.
//!
//! [Ratatui]: https://ratatui.rs

use ratatui_core::text::{Line, Text};

#[cfg(feature = "front-matter")]
pub mod front_matter;
#[cfg(feature = "kramdown")]
mod kramdown;
mod render;
#[cfg(feature = "syntax")]
mod syntax;
mod table;
mod theme;
mod view;
mod wrap;
mod writer;

pub use theme::Theme;
pub use view::{MarkdownState, MarkdownView};

/// Lays `markdown` out in lines at most `width` columns wide.
///
/// With the `front-matter` feature, front matter at the top is split off
/// first and returned in [`Rendered::front_matter`].
pub fn render(markdown: &str, width: u16, theme: &Theme) -> Rendered {
    #[cfg(feature = "front-matter")]
    {
        let (front_matter, body) = front_matter::FrontMatter::split(markdown);
        Rendered {
            front_matter,
            ..render::Renderer::new(theme, width).run(body)
        }
    }
    #[cfg(not(feature = "front-matter"))]
    render::Renderer::new(theme, width).run(markdown)
}

/// A rendered document.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Rendered {
    /// The document's lines, already wrapped. Draw them without wrapping.
    pub lines: Vec<Line<'static>>,
    /// Every link, in the order they appear.
    pub links: Vec<Link>,
    /// The front matter between `---` or `+++` lines at the top.
    #[cfg(feature = "front-matter")]
    pub front_matter: Option<front_matter::FrontMatter>,
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
