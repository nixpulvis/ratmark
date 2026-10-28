//! Word wrapping for runs of styled text.

use ratatui_core::style::Style;
use unicode_width::UnicodeWidthStr;

/// A run of text in one style, possibly part of a link.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Piece {
    pub text: String,
    pub style: Style,
    pub link: Option<usize>,
}

impl Piece {
    pub fn new(text: impl Into<String>, style: Style) -> Self {
        Self {
            text: text.into(),
            style,
            link: None,
        }
    }

    pub fn width(&self) -> usize {
        self.text.width()
    }
}

/// Inline content waiting to be laid out.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Inline {
    Piece(Piece),
    /// A hard line break.
    Break,
}

enum Token {
    Word(Vec<Piece>),
    Space(Piece),
    Break,
}

/// Splits inline content into words, spaces and breaks. Runs of whitespace
/// collapse to a single space, like HTML.
fn tokenize(inline: &[Inline]) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut word: Vec<Piece> = Vec::new();
    for item in inline {
        let piece = match item {
            Inline::Piece(piece) => piece,
            Inline::Break => {
                if !word.is_empty() {
                    tokens.push(Token::Word(std::mem::take(&mut word)));
                }
                tokens.push(Token::Break);
                continue;
            }
        };
        let mut rest = piece.text.as_str();
        while !rest.is_empty() {
            let split = rest.find(char::is_whitespace).unwrap_or(rest.len());
            if split > 0 {
                word.push(Piece {
                    text: rest[..split].to_owned(),
                    ..piece.clone()
                });
            }
            rest = &rest[split..];
            let spaces = rest.len() - rest.trim_start().len();
            if spaces > 0 {
                if !word.is_empty() {
                    tokens.push(Token::Word(std::mem::take(&mut word)));
                }
                if !matches!(tokens.last(), Some(Token::Space(_))) {
                    tokens.push(Token::Space(Piece {
                        text: " ".into(),
                        ..piece.clone()
                    }));
                }
                rest = &rest[spaces..];
            }
        }
    }
    if !word.is_empty() {
        tokens.push(Token::Word(word));
    }
    tokens
}

/// Lays inline content out in lines at most `width` columns wide. Words wider
/// than a whole line are split between characters.
pub(crate) fn wrap(inline: &[Inline], width: usize) -> Vec<Vec<Piece>> {
    let width = width.max(1);
    let mut lines = Vec::new();
    let mut line: Vec<Piece> = Vec::new();
    let mut used = 0;
    let mut space: Option<Piece> = None;

    for token in tokenize(inline) {
        match token {
            Token::Break => {
                lines.push(std::mem::take(&mut line));
                used = 0;
                space = None;
            }
            Token::Space(piece) => space = Some(piece),
            Token::Word(pieces) => {
                let word: usize = pieces.iter().map(Piece::width).sum();
                if used > 0 {
                    if used + 1 + word <= width {
                        line.extend(space.take());
                        used += 1;
                    } else {
                        lines.push(std::mem::take(&mut line));
                        used = 0;
                    }
                }
                space = None;
                for piece in pieces {
                    if used + piece.width() <= width {
                        used += piece.width();
                        line.push(piece);
                        continue;
                    }
                    // Too long for any line, so break it wherever it fills up.
                    let mut part = String::new();
                    for c in piece.text.chars() {
                        let w = unicode_width::UnicodeWidthChar::width(c).unwrap_or(0);
                        if used + w > width && used > 0 {
                            if !part.is_empty() {
                                line.push(Piece {
                                    text: std::mem::take(&mut part),
                                    ..piece.clone()
                                });
                            }
                            lines.push(std::mem::take(&mut line));
                            used = 0;
                        }
                        part.push(c);
                        used += w;
                    }
                    if !part.is_empty() {
                        line.push(Piece {
                            text: part,
                            ..piece
                        });
                    }
                }
            }
        }
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(lines: &[Vec<Piece>]) -> Vec<String> {
        lines
            .iter()
            .map(|l| l.iter().map(|p| p.text.as_str()).collect())
            .collect()
    }

    fn plain(s: &str) -> Vec<Inline> {
        vec![Inline::Piece(Piece::new(s, Style::new()))]
    }

    #[test]
    fn wraps_at_spaces() {
        let lines = wrap(&plain("the quick brown fox jumps"), 10);
        assert_eq!(text(&lines), ["the quick", "brown fox", "jumps"]);
    }

    #[test]
    fn collapses_whitespace() {
        let lines = wrap(&plain("  a   b \t c  "), 80);
        assert_eq!(text(&lines), ["a b c"]);
    }

    #[test]
    fn splits_long_words() {
        let lines = wrap(&plain("abcdefghij xy"), 4);
        assert_eq!(text(&lines), ["abcd", "efgh", "ij", "xy"]);
    }

    #[test]
    fn keeps_styled_runs_in_one_word() {
        let bold = Style::new().bold();
        let inline = vec![
            Inline::Piece(Piece::new("foo", Style::new())),
            Inline::Piece(Piece::new("bar baz", bold)),
        ];
        let lines = wrap(&inline, 6);
        assert_eq!(text(&lines), ["foobar", "baz"]);
        assert_eq!(lines[0][1].style, bold);
    }

    #[test]
    fn breaks() {
        let mut inline = plain("a");
        inline.push(Inline::Break);
        inline.extend(plain("b"));
        assert_eq!(text(&wrap(&inline, 80)), ["a", "b"]);
    }
}
