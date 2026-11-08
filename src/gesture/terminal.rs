//! Following a terminal's mouse wheel with [`Gestures`].
//!
//! A terminal reports the wheel a tick at a time, up, down, left or right,
//! and a trackpad swipe that's a little diagonal sends ticks both ways. A
//! [`Wheel`] keeps each gesture going one way, so the swipe either scrolls
//! or shifts the wide code under the pointer, not both.
//!
//! It counts time with std's [`Instant`], so it isn't built for
//! `wasm32-unknown-unknown`, which has no clock for it. In a browser, use
//! the `web` feature's `web::listen` instead.

use std::time::Instant;

use super::{Gestures, Step};

/// A terminal's mouse wheel, moving the view a few cells a tick, one way a
/// gesture.
///
/// The terminal's events are the host's to read, so each gives a `match`
/// from its wheel events to ticks, like this one for crossterm's:
///
/// ```
/// use ratmark::MarkdownState;
/// use ratmark::gesture::terminal::Wheel;
/// use ratatui_crossterm::crossterm::event::{MouseEvent, MouseEventKind};
/// # use ratatui_crossterm::crossterm::event::KeyModifiers;
/// # use ratatui_core::{buffer::Buffer, layout::Rect, widgets::StatefulWidget};
///
/// /// How far a crossterm wheel event goes, right and down, in ticks.
/// fn ticks(kind: MouseEventKind) -> Option<(isize, isize)> {
///     match kind {
///         MouseEventKind::ScrollDown => Some((0, 1)),
///         MouseEventKind::ScrollUp => Some((0, -1)),
///         MouseEventKind::ScrollRight => Some((1, 0)),
///         MouseEventKind::ScrollLeft => Some((-1, 0)),
///         _ => None,
///     }
/// }
///
/// let markdown = (1..=20).map(|n| format!("{n}\n\n")).collect::<String>();
/// let mut state = MarkdownState::new(markdown);
/// # let area = Rect::new(0, 0, 10, 5);
/// # ratmark::MarkdownView::new(&Default::default())
/// #     .render(area, &mut Buffer::empty(area), &mut state);
/// // Three lines, or columns, a tick.
/// let mut wheel = Wheel::new(3);
///
/// // On each mouse event.
/// # let mouse = MouseEvent {
/// #     kind: MouseEventKind::ScrollDown,
/// #     column: 4,
/// #     row: 2,
/// #     modifiers: KeyModifiers::NONE,
/// # };
/// if let Some(ticks) = ticks(mouse.kind) {
///     wheel.tick(mouse.column, mouse.row, ticks, |step| step.apply(&mut state));
/// }
/// assert_eq!(state.scroll(), 3);
/// ```
#[derive(Debug, Clone)]
pub struct Wheel {
    gestures: Gestures,
    /// What the gestures' times count from.
    start: Instant,
    /// How many lines, or columns, a tick moves.
    cells: f64,
}

impl Wheel {
    /// A wheel moving `cells` lines, or columns, a tick.
    pub fn new(cells: u16) -> Self {
        Self {
            // Ticks and the pointer are in whole cells already.
            gestures: Gestures::new(1.0, 1.0),
            start: Instant::now(),
            cells: f64::from(cells),
        }
    }

    /// Ticks over `column`, `row`, going `ticks.0` right and `ticks.1` down,
    /// handing each [`Step`] to `step`, which carries it out and returns
    /// whether it moved. Each gesture goes the way its first tick does,
    /// until a pause, or until it runs into the end.
    pub fn tick(
        &mut self,
        column: u16,
        row: u16,
        ticks: (isize, isize),
        step: impl FnOnce(Step) -> bool,
    ) {
        let time = self.start.elapsed().as_secs_f64() * 1000.0;
        self.tick_at(column, row, ticks, time, step);
    }

    fn tick_at(
        &mut self,
        column: u16,
        row: u16,
        (x, y): (isize, isize),
        time: f64,
        step: impl FnOnce(Step) -> bool,
    ) {
        let at = (f64::from(column), f64::from(row));
        let delta = (x as f64 * self.cells, y as f64 * self.cells);
        self.gestures.wheel(at, delta, time, step);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gesture::Axis;

    #[test]
    fn ticks_move_one_way_over_the_pointer() {
        let mut wheel = Wheel::new(3);
        let mut steps = Vec::new();
        let mut record = |step: Step| {
            steps.push(step);
            true
        };
        wheel.tick_at(7, 4, (1, 0), 0.0, &mut record);
        // A diagonal swipe's tick the other way, in the same gesture.
        wheel.tick_at(7, 4, (0, 1), 20.0, &mut record);
        wheel.tick_at(7, 4, (-1, 0), 40.0, &mut record);
        // After a pause, a new gesture can go the other way.
        wheel.tick_at(7, 4, (0, 1), 500.0, &mut record);
        let step = |axis, cells| Step {
            axis,
            cells,
            column: 7,
            row: 4,
        };
        assert_eq!(
            steps,
            [
                step(Axis::Horizontal, 3),
                step(Axis::Horizontal, -3),
                step(Axis::Vertical, 3),
            ]
        );
    }
}
