//! Syntax highlighting for code blocks, with syntect.

use std::sync::LazyLock;

use ratatui_core::style::{Color, Style};
use syntect::easy::HighlightLines;
use syntect::highlighting::{self, StyleModifier, ThemeItem, ThemeSet, ThemeSettings};
use syntect::parsing::SyntaxSet;
use syntect::util::LinesWithEndings;

use crate::Theme;
use crate::wrap::Piece;

impl Theme {
    /// Colors `code` as the language named `lang`, one entry per line,
    /// with the syntax theme over `base`. Returns `None` when the theme has
    /// no syntax theme, or when the language or the syntax theme isn't
    /// known.
    pub(crate) fn highlight(&self, code: &str, lang: &str, base: Style) -> Option<Vec<Vec<Piece>>> {
        let syntax = SYNTAXES.find_syntax_by_token(lang)?;
        let colors = THEMES.themes.get(self.syntax?)?;
        let mut highlighter = HighlightLines::new(syntax, colors);
        let mut lines = Vec::new();
        for line in LinesWithEndings::from(code) {
            let ranges = highlighter.highlight_line(line, &SYNTAXES).ok()?;
            lines.push(
                ranges
                    .into_iter()
                    .map(|(style, text)| {
                        let c = style.foreground;
                        let style = match c.a {
                            0 => base.fg(self.syntax_color(c.r)),
                            1 => base,
                            _ => base.fg(Color::Rgb(c.r, c.g, c.b)),
                        };
                        Piece::new(text.trim_end_matches('\n'), style)
                    })
                    .filter(|piece| !piece.text.is_empty())
                    .collect(),
            );
        }
        Some(lines)
    }

    /// Terminal color number `n`, in [`Theme::syntax_palette`] when it has
    /// one for it.
    fn syntax_color(&self, n: u8) -> Color {
        self.syntax_palette
            .as_ref()
            .and_then(|palette| palette.get(usize::from(n)))
            .copied()
            .unwrap_or(Color::Indexed(n))
    }
}

static SYNTAXES: LazyLock<SyntaxSet> = LazyLock::new(SyntaxSet::load_defaults_newlines);

static THEMES: LazyLock<ThemeSet> = LazyLock::new(|| {
    let mut set = ThemeSet::load_defaults();
    set.themes.insert("ansi".into(), ansi());
    set
});

/// Syntax colors as terminal color numbers, so the terminal's palette
/// decides what they look like: comments muted gray, constants green,
/// keywords red, functions blue, types bright yellow and macros magenta.
/// Everything else keeps the code block's foreground.
fn ansi() -> highlighting::Theme {
    // syntect colors are RGBA. An alpha of 00 makes the red a terminal color
    // number, and 01 keeps the foreground, as `Theme::highlight` reads them.
    let color = |n| highlighting::Color {
        r: n,
        g: 0,
        b: 0,
        a: 0,
    };
    let scopes = [
        ("comment", 8),
        ("string, constant", 2),
        ("keyword, storage", 1),
        (
            "entity.name.function, support.function, variable.function",
            4,
        ),
        // Types, like `i32` and C's `int`. syntect also scopes the keywords
        // that declare things as `storage.type`, so those are taken back
        // below, but Rust's `let` can't be told from `i32` and stays a type.
        (
            "entity.name.type, entity.name.struct, entity.name.enum, entity.name.union, entity.name.trait, support.type, storage.type",
            11,
        ),
        (
            "storage.type.function, storage.type.struct, storage.type.enum, storage.type.union, storage.type.trait, storage.type.impl, storage.type.class, storage.type.module",
            1,
        ),
        (
            "meta.preprocessor, meta.annotation, support.macro, entity.name.macro, keyword.control.import",
            5,
        ),
        ("invalid", 9),
    ];
    highlighting::Theme {
        name: Some("ansi".into()),
        settings: ThemeSettings {
            foreground: Some(highlighting::Color {
                r: 0,
                g: 0,
                b: 0,
                a: 1,
            }),
            ..ThemeSettings::default()
        },
        scopes: scopes
            .into_iter()
            .map(|(scope, n)| ThemeItem {
                scope: scope.parse().expect("ansi scope selectors parse"),
                style: StyleModifier {
                    foreground: Some(color(n)),
                    ..StyleModifier::default()
                },
            })
            .collect(),
        ..highlighting::Theme::default()
    }
}
