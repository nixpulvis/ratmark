//! A full-screen Markdown reader, built on [`MarkdownView`], that scrolls,
//! follows links and shifts wide code sideways.
//!
//!     cargo run --example tui -- [FILE]
//!
//! Without a file it shows a short guide. Turn on the features a file needs,
//! like `syntax` to highlight its code, or `html` and `kramdown` for pages
//! from a Jekyll site.

use std::io::{self, Stdout};
use std::{env, fs, panic};

use ratatui_core::layout::{Constraint, Layout};
use ratatui_core::style::Color;
use ratatui_core::terminal::{Frame, Terminal};
use ratatui_core::text::Line;
use ratatui_crossterm::CrosstermBackend;
use ratatui_crossterm::crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind, MouseButton,
    MouseEvent, MouseEventKind,
};
use ratatui_crossterm::crossterm::execute;
use ratatui_crossterm::crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratmark::{MarkdownState, MarkdownView, Theme};

/// The widest the text gets, for comfortable reading on wide terminals.
const MAX_WIDTH: u16 = 80;

/// How far the mouse wheel scrolls, in lines.
const WHEEL: isize = 3;

/// How far wide code shifts sideways per key press, in columns.
const SHIFT: isize = 4;

/// Gruvbox's dark background. The theme leaves the background to the
/// terminal, but its colors are made for this one.
const BACKGROUND: Color = Color::Rgb(0x28, 0x28, 0x28);

/// Shown at the bottom when there's nothing else to say.
const HELP: &str = "tab links · enter follow · q quit";

const GUIDE: &str = r#"# ratmark

A Markdown reader for the terminal, drawn by `MarkdownView`. Pass it a file
to read that instead.

## Keys

| Key               | Does                            |
|-------------------|---------------------------------|
| `↑` `↓`, wheel    | Scroll                          |
| `PgUp` `PgDn`     | Scroll a page                   |
| `←` `→`           | Shift wide code sideways        |
| `tab` `shift-tab` | Focus the next or previous link |
| `enter`, click    | Follow a link                   |
| `q`               | Quit                            |
"#;

fn main() -> io::Result<()> {
    let markdown = match env::args().nth(1) {
        Some(path) => fs::read_to_string(path)?,
        None => GUIDE.to_owned(),
    };
    let theme = Theme::default();
    App {
        theme: Theme {
            text: theme.text.bg(BACKGROUND),
            ..theme
        },
        state: MarkdownState::new(markdown),
        message: None,
    }
    .start()
}

struct App {
    theme: Theme,
    state: MarkdownState,
    /// Shown at the bottom until the next key press.
    message: Option<String>,
}

impl App {
    /// Takes over the terminal, raw input, the alternate screen and the
    /// mouse, runs until `q`, and gives the terminal back, even on a panic.
    fn start(mut self) -> io::Result<()> {
        let hook = panic::take_hook();
        panic::set_hook(Box::new(move |info| {
            let _ = Self::stop();
            hook(info);
        }));
        enable_raw_mode()?;
        execute!(io::stdout(), EnterAlternateScreen, EnableMouseCapture)?;
        let result = Terminal::new(CrosstermBackend::new(io::stdout()))
            .and_then(|mut terminal| self.run(&mut terminal));
        Self::stop()?;
        result
    }

    /// Gives the terminal back the way it was.
    fn stop() -> io::Result<()> {
        execute!(io::stdout(), DisableMouseCapture, LeaveAlternateScreen)?;
        disable_raw_mode()
    }

    fn run(&mut self, terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> io::Result<()> {
        loop {
            terminal.draw(|frame| self.draw(frame))?;
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => {
                    if key.code == KeyCode::Char('q') {
                        return Ok(());
                    }
                    self.key(key.code);
                }
                Event::Mouse(mouse) => self.mouse(mouse),
                _ => {}
            }
        }
    }

    /// Draws the document, with a status line under it.
    fn draw(&mut self, frame: &mut Frame) {
        let [body, status] =
            Layout::vertical([Constraint::Fill(1), Constraint::Length(1)]).areas(frame.area());

        frame.buffer_mut().set_style(body, self.theme.text);
        let page = body.centered_horizontally(Constraint::Max(MAX_WIDTH));
        frame.render_stateful_widget(MarkdownView::new(&self.theme), page, &mut self.state);

        let note = self
            .message
            .as_deref()
            .or(self.state.focused().map(|link| link.url.as_str()))
            .unwrap_or(HELP);
        let line = Line::styled(format!(" {note}"), self.theme.code_block);
        frame.render_widget(line, status);
    }

    fn key(&mut self, code: KeyCode) {
        self.message = None;
        let state = &mut self.state;
        match code {
            KeyCode::Down => _ = state.scroll_by(1),
            KeyCode::Up => _ = state.scroll_by(-1),
            KeyCode::PageDown => _ = state.page_by(1),
            KeyCode::PageUp => _ = state.page_by(-1),
            KeyCode::Right => _ = state.shift_by(SHIFT),
            KeyCode::Left => _ = state.shift_by(-SHIFT),
            KeyCode::Tab => state.focus_next(),
            KeyCode::BackTab => state.focus_prev(),
            KeyCode::Enter => {
                if let Some(url) = state.focused().map(|link| link.url.clone()) {
                    self.follow(&url);
                }
            }
            _ => {}
        }
    }

    fn mouse(&mut self, mouse: MouseEvent) {
        let state = &mut self.state;
        match mouse.kind {
            MouseEventKind::ScrollDown => _ = state.scroll_by(WHEEL),
            MouseEventKind::ScrollUp => _ = state.scroll_by(-WHEEL),
            MouseEventKind::Down(MouseButton::Left) => {
                if let Some(link) = state.link_at(mouse.column, mouse.row) {
                    let url = link.url.clone();
                    self.follow(&url);
                }
            }
            _ => {}
        }
    }

    /// Scrolls to a `#section` link's heading. Other links are the app's to
    /// open, in a browser say; this one only shows where they go.
    fn follow(&mut self, url: &str) {
        match url.strip_prefix('#') {
            Some(anchor) => self.state.scroll_to_anchor(anchor),
            None => self.message = Some(format!("Would open {url}")),
        }
    }
}
