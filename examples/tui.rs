//! A full-screen Markdown reader, built on [`MarkdownView`], that scrolls,
//! follows links and shifts wide code sideways.
//!
//!     cargo run --example tui -- [FILE]
//!
//! Without a file it shows a short guide. Turn on the features a file needs,
//! like `syntax` to highlight its code, or `html` and `kramdown` for pages
//! from a Jekyll site.

use std::io::{self, Stdout};
use std::time::Duration;
use std::{env, fs, panic};

use ratatui_core::layout::{Constraint, Layout};
use ratatui_core::terminal::{Frame, Terminal};
use ratatui_core::text::Line;
use ratatui_crossterm::CrosstermBackend;
use ratatui_crossterm::crossterm::event::{
    self, DisableFocusChange, DisableMouseCapture, EnableFocusChange, EnableMouseCapture, Event,
    KeyCode, KeyEventKind, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui_crossterm::crossterm::execute;
use ratatui_crossterm::crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratmark::gesture::terminal::Wheel;
use ratmark::{MarkdownState, MarkdownView, Theme};
use terminal_colorsaurus::{QueryOptions, ThemeMode, theme_mode};

/// The widest the text gets, for comfortable reading on wide terminals.
const MAX_WIDTH: u16 = 80;

/// How far a tick of the mouse wheel scrolls, in lines, or shifts code, in
/// columns.
const WHEEL: u16 = 3;

/// How far wide code shifts sideways per key press, in columns.
const SHIFT: isize = 4;

/// How long input has to be quiet before the terminal is asked for its
/// background again, to follow a switch between light and dark.
const RECHECK: Duration = Duration::from_secs(2);

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
| `←` `→`, wheel    | Shift wide code sideways        |
| `tab` `shift-tab` | Focus the next or previous link |
| `enter`, click    | Follow a link                   |
| `q`               | Quit                            |
"#;

fn main() -> io::Result<()> {
    let markdown = match env::args().nth(1) {
        Some(path) => fs::read_to_string(path)?,
        None => GUIDE.to_owned(),
    };
    // Ask the terminal for its background before raw mode, so the reply
    // isn't read as input.
    App {
        theme: background_theme(),
        state: MarkdownState::new(markdown),
        wheel: Wheel::new(WHEEL),
        message: None,
    }
    .start()
}

/// The theme for the terminal's background, dark when the terminal doesn't
/// say.
///
/// The reply is read straight from the terminal, so call this only when
/// crossterm isn't reading events. Input typed while it waits for the reply
/// can be lost.
fn background_theme() -> Theme {
    match theme_mode(QueryOptions::default()) {
        Ok(ThemeMode::Light) => Theme::ansi_light(),
        _ => Theme::ansi_dark(),
    }
}

struct App {
    theme: Theme,
    state: MarkdownState,
    wheel: Wheel,
    /// Shown at the bottom until the next key press.
    message: Option<String>,
}

impl App {
    /// Takes over the terminal, raw input, the alternate screen, the mouse
    /// and focus reports, runs until `q`, and gives the terminal back, even
    /// on a panic.
    fn start(mut self) -> io::Result<()> {
        let hook = panic::take_hook();
        panic::set_hook(Box::new(move |info| {
            let _ = Self::stop();
            hook(info);
        }));
        enable_raw_mode()?;
        execute!(
            io::stdout(),
            EnterAlternateScreen,
            EnableMouseCapture,
            EnableFocusChange
        )?;
        let result = Terminal::new(CrosstermBackend::new(io::stdout()))
            .and_then(|mut terminal| self.run(&mut terminal));
        Self::stop()?;
        result
    }

    /// Gives the terminal back the way it was.
    fn stop() -> io::Result<()> {
        execute!(
            io::stdout(),
            DisableFocusChange,
            DisableMouseCapture,
            LeaveAlternateScreen
        )?;
        disable_raw_mode()
    }

    fn run(&mut self, terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> io::Result<()> {
        loop {
            terminal.draw(|frame| self.draw(frame))?;
            // When input has been quiet a while, or the window comes back
            // into focus, ask for the background again, so the theme follows
            // the terminal between light and dark.
            if !event::poll(RECHECK)? {
                self.recheck_background();
                continue;
            }
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => {
                    if key.code == KeyCode::Char('q') {
                        return Ok(());
                    }
                    self.key(key.code);
                }
                Event::Mouse(mouse) => self.mouse(mouse),
                Event::FocusGained => self.recheck_background(),
                _ => {}
            }
        }
    }

    /// Switches to the theme for the terminal's background.
    fn recheck_background(&mut self) {
        self.theme = background_theme();
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
        let ticks = match mouse.kind {
            MouseEventKind::ScrollDown => (0, 1),
            MouseEventKind::ScrollUp => (0, -1),
            // Sideways, the wide code block under the pointer.
            MouseEventKind::ScrollRight => (1, 0),
            MouseEventKind::ScrollLeft => (-1, 0),
            MouseEventKind::Down(MouseButton::Left) => {
                if let Some(link) = self.state.link_at(mouse.column, mouse.row) {
                    let url = link.url.clone();
                    self.follow(&url);
                }
                return;
            }
            _ => return,
        };
        let state = &mut self.state;
        self.wheel
            .tick(mouse.column, mouse.row, ticks, |step| step.apply(state));
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
