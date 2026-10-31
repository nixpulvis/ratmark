//! Prints a document to the terminal the way [`MarkdownView`] draws it,
//! colors and all, for the examples. A real app would draw the view in a
//! Ratatui frame instead.

use std::fmt::Write;

use ratatui_core::buffer::Buffer;
use ratatui_core::layout::Rect;
use ratatui_core::style::{Color, Modifier};
use ratatui_core::widgets::StatefulWidget;
use ratmark::{MarkdownState, MarkdownView, Rendered, Theme};

/// How wide the examples draw.
pub const WIDTH: u16 = 60;

/// Draws the whole of `state`'s document, `WIDTH` columns wide, and prints
/// it with ANSI colors.
pub fn print(state: &mut MarkdownState, theme: &Theme) {
    let height = Rendered::render(state.markdown(), WIDTH, theme).lines.len() as u16;
    let area = Rect::new(0, 0, WIDTH, height);
    let mut buf = Buffer::empty(area);
    MarkdownView::new(theme).render(area, &mut buf, state);
    for y in 0..height {
        let mut line = String::new();
        #[cfg(feature = "native-links")]
        let mut link = None;
        for x in 0..WIDTH {
            let cell = &buf[(x, y)];
            // Real links, as OSC 8, in terminals that support them. The id
            // keeps the pieces of a wrapped link one link.
            #[cfg(feature = "native-links")]
            if cell.hyperlink() != link {
                link = cell.hyperlink();
                match link {
                    Some(h) => {
                        let id = h.id().unwrap_or_default();
                        let _ = write!(line, "\x1b]8;id={id};{}\x1b\\", h.uri());
                    }
                    None => line.push_str("\x1b]8;;\x1b\\"),
                }
            }
            let _ = write!(line, "\x1b[0{}{}", sgr(cell.fg, 30), sgr(cell.bg, 40));
            for (modifier, code) in [
                (Modifier::BOLD, 1),
                (Modifier::ITALIC, 3),
                (Modifier::UNDERLINED, 4),
                (Modifier::REVERSED, 7),
                (Modifier::CROSSED_OUT, 9),
            ] {
                if cell.modifier.contains(modifier) {
                    let _ = write!(line, ";{code}");
                }
            }
            line.push('m');
            line.push_str(cell.symbol());
        }
        #[cfg(feature = "native-links")]
        if link.is_some() {
            line.push_str("\x1b]8;;\x1b\\");
        }
        println!("{line}\x1b[0m");
    }
}

/// The SGR parameters for `color`, where `base` is 30 for the foreground and
/// 40 for the background.
fn sgr(color: Color, base: u8) -> String {
    let named = |i: u8| format!(";{}", base + i);
    let bright = |i: u8| format!(";{}", base + 60 + i);
    match color {
        Color::Reset => String::new(),
        Color::Black => named(0),
        Color::Red => named(1),
        Color::Green => named(2),
        Color::Yellow => named(3),
        Color::Blue => named(4),
        Color::Magenta => named(5),
        Color::Cyan => named(6),
        Color::Gray => named(7),
        Color::DarkGray => bright(0),
        Color::LightRed => bright(1),
        Color::LightGreen => bright(2),
        Color::LightYellow => bright(3),
        Color::LightBlue => bright(4),
        Color::LightMagenta => bright(5),
        Color::LightCyan => bright(6),
        Color::White => bright(7),
        Color::Indexed(i) => format!(";{};5;{i}", base + 8),
        Color::Rgb(r, g, b) => format!(";{};2;{r};{g};{b}", base + 8),
    }
}
