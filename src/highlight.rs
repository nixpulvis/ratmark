//! Syntax highlighting for code blocks, with syntect.

use std::sync::OnceLock;

use ratatui_core::style::Color;
pub use syntect;
use syntect::easy::HighlightLines;
use syntect::highlighting::ThemeSet;
use syntect::parsing::SyntaxSet;
use syntect::util::LinesWithEndings;

static SYNTAXES: OnceLock<SyntaxSet> = OnceLock::new();
static THEMES: OnceLock<ThemeSet> = OnceLock::new();

/// Highlights code with these syntaxes and themes, instead of syntect's
/// defaults. A smaller set, like only the languages a site uses, keeps a
/// wasm build small. Call it before rendering, since the first render
/// settles which sets are used. Returns `false` if that already happened.
pub fn set(syntaxes: SyntaxSet, themes: ThemeSet) -> bool {
    let set = SYNTAXES.set(syntaxes).is_ok();
    THEMES.set(themes).is_ok() && set
}

fn syntaxes() -> Option<&'static SyntaxSet> {
    #[cfg(feature = "default-syntaxes")]
    return Some(SYNTAXES.get_or_init(SyntaxSet::load_defaults_newlines));
    #[cfg(not(feature = "default-syntaxes"))]
    SYNTAXES.get()
}

fn themes() -> Option<&'static ThemeSet> {
    #[cfg(feature = "default-syntaxes")]
    return Some(THEMES.get_or_init(ThemeSet::load_defaults));
    #[cfg(not(feature = "default-syntaxes"))]
    THEMES.get()
}

/// Colors `code` as the language named `lang`, one entry per line. Returns
/// `None` when the language or theme is unknown.
pub(crate) fn highlight(code: &str, lang: &str, theme: &str) -> Option<Vec<Vec<(String, Color)>>> {
    let syntaxes = syntaxes()?;
    let syntax = syntaxes.find_syntax_by_token(lang)?;
    let theme = themes()?.themes.get(theme)?;
    let mut highlighter = HighlightLines::new(syntax, theme);
    let mut lines = Vec::new();
    for line in LinesWithEndings::from(code) {
        let ranges = highlighter.highlight_line(line, syntaxes).ok()?;
        lines.push(
            ranges
                .into_iter()
                .map(|(style, text)| {
                    let c = style.foreground;
                    (
                        text.trim_end_matches('\n').to_owned(),
                        Color::Rgb(c.r, c.g, c.b),
                    )
                })
                .filter(|(text, _)| !text.is_empty())
                .collect(),
        );
    }
    Some(lines)
}
