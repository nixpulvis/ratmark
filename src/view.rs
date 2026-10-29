//! A scrolling document view, with links that can be focused and clicked.

use ratatui_core::buffer::Buffer;
use ratatui_core::layout::Rect;
use ratatui_core::style::{Modifier, Style};
use ratatui_core::widgets::StatefulWidget;

use crate::{Link, Rendered, Theme};

/// Draws a [`MarkdownState`]'s document, laid out to fit the area it's
/// given, scrolled and with its focused link highlighted.
///
/// ```
/// use ratmark::{MarkdownState, MarkdownView, Theme};
/// # use ratatui_core::{buffer::Buffer, layout::Rect, widgets::StatefulWidget};
///
/// let mut state = MarkdownState::new("See [the docs](https://docs.rs).");
/// let theme = Theme::default();
/// # let area = Rect::new(0, 0, 40, 5);
/// # let mut buf = Buffer::empty(area);
/// MarkdownView::new(&theme).render(area, &mut buf, &mut state);
/// // frame.render_stateful_widget(MarkdownView::new(&theme), area, &mut state);
///
/// state.focus_next();
/// assert_eq!(state.focused().map(|l| l.url.as_str()), Some("https://docs.rs"));
/// assert_eq!(state.link_at(4, 0).map(|l| l.url.as_str()), Some("https://docs.rs"));
/// ```
#[derive(Debug, Clone)]
pub struct MarkdownView<'t> {
    theme: &'t Theme,
    focus: Style,
}

impl<'t> MarkdownView<'t> {
    pub fn new(theme: &'t Theme) -> Self {
        Self {
            theme,
            focus: Style::new().add_modifier(Modifier::REVERSED),
        }
    }

    /// The style patched over the focused link. Reversed colors by default.
    pub fn focus_style(mut self, style: Style) -> Self {
        self.focus = style;
        self
    }
}

/// A document and where it's scrolled to, for [`MarkdownView`].
#[derive(Debug, Clone, Default)]
pub struct MarkdownState {
    markdown: String,
    /// The layout, and the width it was made for.
    rendered: Option<(u16, Rendered)>,
    scroll: usize,
    focus: Option<usize>,
    /// Where the view was last drawn.
    area: Rect,
    /// A `#section` to scroll to once laid out.
    anchor: Option<String>,
}

impl MarkdownState {
    pub fn new(markdown: impl Into<String>) -> Self {
        Self {
            markdown: markdown.into(),
            ..Self::default()
        }
    }

    /// The Markdown being shown.
    pub fn markdown(&self) -> &str {
        &self.markdown
    }

    /// The document as last laid out, once it has been drawn.
    pub fn rendered(&self) -> Option<&Rendered> {
        self.rendered.as_ref().map(|(_, r)| r)
    }

    /// The first line on screen.
    pub fn scroll(&self) -> usize {
        self.scroll
    }

    /// Scrolls so `line` is at the top, as far as the document allows.
    pub fn scroll_to(&mut self, line: usize) {
        self.scroll = line;
        self.clamp();
    }

    /// Scrolls by `lines`, down when positive.
    pub fn scroll_by(&mut self, lines: isize) {
        self.scroll_to(self.scroll.saturating_add_signed(lines));
    }

    /// Scrolls by a screenful, less a couple of lines for context.
    pub fn page(&mut self, down: bool) {
        let lines = (self.area.height as isize - 2).max(1);
        self.scroll_by(if down { lines } else { -lines });
    }

    /// Scrolls to the heading a `#section` link points at, now or once the
    /// document is laid out. Matching ignores case and punctuation, since
    /// tools make heading ids a little differently.
    pub fn scroll_to_anchor(&mut self, anchor: &str) {
        self.anchor = Some(anchor.to_owned());
        self.apply_anchor();
    }

    /// How far down the document can scroll.
    fn max_scroll(&self) -> usize {
        let lines = self.rendered().map_or(0, |r| r.lines.len());
        lines.saturating_sub(usize::from(self.area.height))
    }

    fn clamp(&mut self) {
        if self.rendered.is_some() {
            self.scroll = self.scroll.min(self.max_scroll());
        }
    }

    fn apply_anchor(&mut self) {
        let Some((_, rendered)) = &self.rendered else {
            return;
        };
        let Some(anchor) = self.anchor.take() else {
            return;
        };
        if let Some(h) = rendered
            .headings
            .iter()
            .find(|h| anchor_matches(&h.text, &anchor))
        {
            self.scroll_to(h.line);
        }
    }

    /// The focused link.
    pub fn focused(&self) -> Option<&Link> {
        self.rendered()?.links.get(self.focus?)
    }

    /// Focuses the next link, starting from the top of the screen, and
    /// scrolls to it.
    pub fn focus_next(&mut self) {
        self.move_focus(true);
    }

    /// Focuses the previous link, starting from the bottom of the screen,
    /// and scrolls to it.
    pub fn focus_prev(&mut self) {
        self.move_focus(false);
    }

    pub fn clear_focus(&mut self) {
        self.focus = None;
    }

    fn move_focus(&mut self, forward: bool) {
        let height = usize::from(self.area.height.max(1));
        let scroll = self.scroll;
        let Some(rendered) = self.rendered() else {
            return;
        };
        let count = rendered.links.len();
        if count == 0 {
            return;
        }
        let line = |i: usize| rendered.links[i].regions.first().map_or(0, |r| r.line);
        let visible = |i: usize| (scroll..scroll + height).contains(&line(i));
        let next = match self.focus {
            // Keep going from a focused link that's still on screen.
            Some(i) if visible(i) => {
                if forward {
                    (i + 1) % count
                } else {
                    (i + count - 1) % count
                }
            }
            // Otherwise start from what's on screen, so a long page doesn't
            // jump back to the top.
            _ => {
                let on_screen = (0..count).filter(|&i| visible(i));
                let found = if forward {
                    on_screen.min()
                } else {
                    on_screen.max()
                };
                found.unwrap_or_else(|| {
                    let below = (0..count).find(|&i| line(i) >= scroll + height);
                    let above = (0..count).rev().find(|&i| line(i) < scroll);
                    if forward {
                        below.unwrap_or(0)
                    } else {
                        above.unwrap_or(count - 1)
                    }
                })
            }
        };
        let at = line(next);
        self.focus = Some(next);
        if at < self.scroll {
            self.scroll_to(at);
        } else if at >= self.scroll + height {
            self.scroll_to(at + 1 - height);
        }
    }

    /// The link drawn at screen position `x`, `y`, if any.
    pub fn link_at(&self, x: u16, y: u16) -> Option<&Link> {
        let area = self.area;
        if x < area.x || y < area.y || x >= area.right() || y >= area.bottom() {
            return None;
        }
        let line = self.scroll + usize::from(y - area.y);
        self.rendered()?.link_at(line, x - area.x)
    }
}

impl StatefulWidget for MarkdownView<'_> {
    type State = MarkdownState;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut MarkdownState) {
        state.area = area;
        if state
            .rendered
            .as_ref()
            .is_none_or(|(w, _)| *w != area.width)
        {
            // Links come out in the same order at any width, so the focus
            // stays put through a resize.
            state.rendered = Some((
                area.width,
                Rendered::render(&state.markdown, area.width, self.theme),
            ));
        }
        state.apply_anchor();
        state.clamp();

        let Some((_, rendered)) = &state.rendered else {
            return;
        };
        let height = usize::from(area.height);
        for (i, line) in rendered
            .lines
            .iter()
            .skip(state.scroll)
            .take(height)
            .enumerate()
        {
            buf.set_line(area.x, area.y + i as u16, line, area.width);
        }
        if let Some(link) = state.focus.and_then(|i| rendered.links.get(i)) {
            for region in &link.regions {
                let Some(row) = region
                    .line
                    .checked_sub(state.scroll)
                    .filter(|r| *r < height)
                else {
                    continue;
                };
                let end = region.end.min(area.width);
                let start = region.start.min(end);
                let rect = Rect::new(area.x + start, area.y + row as u16, end - start, 1);
                buf.set_style(rect, self.focus);
            }
        }
    }
}

/// Loosely matches a heading to a `#section` link.
fn anchor_matches(heading: &str, anchor: &str) -> bool {
    let simplify = |s: &str| -> String {
        s.chars()
            .filter(|c| c.is_alphanumeric())
            .flat_map(char::to_lowercase)
            .collect()
    };
    let heading = simplify(heading);
    let anchor = simplify(anchor);
    !anchor.is_empty() && heading == anchor
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draw(state: &mut MarkdownState, width: u16, height: u16) -> Buffer {
        let area = Rect::new(0, 0, width, height);
        let mut buf = Buffer::empty(area);
        MarkdownView::new(&Theme::plain()).render(area, &mut buf, state);
        buf
    }

    fn row(buf: &Buffer, y: u16) -> String {
        (0..buf.area.width)
            .map(|x| buf[(x, y)].symbol())
            .collect::<String>()
            .trim_end()
            .to_owned()
    }

    #[test]
    fn scrolls_within_the_document() {
        let mut state = MarkdownState::new("a\n\nb\n\nc");
        draw(&mut state, 10, 2);
        state.scroll_by(100);
        assert_eq!(state.scroll(), 3);
        let buf = draw(&mut state, 10, 2);
        assert_eq!((row(&buf, 0), row(&buf, 1)), ("".into(), "c".into()));
        state.scroll_by(-100);
        assert_eq!(state.scroll(), 0);
    }

    #[test]
    fn focus_follows_the_screen() {
        let md = "[a](/a)\n\n[b](/b)\n\nx\n\nx\n\n[c](/c)";
        let mut state = MarkdownState::new(md);
        draw(&mut state, 10, 3);
        state.focus_next();
        assert_eq!(state.focused().unwrap().url, "/a");
        state.scroll_to(6);
        state.focus_next();
        assert_eq!(state.focused().unwrap().url, "/c");
        state.focus_next();
        assert_eq!(state.focused().unwrap().url, "/a");
        assert_eq!(state.scroll(), 0);
    }

    #[test]
    fn focused_link_is_highlighted() {
        let mut state = MarkdownState::new("go [here](/x)");
        draw(&mut state, 20, 1);
        state.focus_next();
        let buf = draw(&mut state, 20, 1);
        assert!(buf[(3, 0)].modifier.contains(Modifier::REVERSED));
        assert!(!buf[(2, 0)].modifier.contains(Modifier::REVERSED));
    }

    #[test]
    fn clicks_find_links() {
        let mut state = MarkdownState::new("go [here](/x)");
        let area = Rect::new(5, 2, 20, 1);
        let mut buf = Buffer::empty(Rect::new(0, 0, 30, 5));
        MarkdownView::new(&Theme::plain()).render(area, &mut buf, &mut state);
        assert_eq!(state.link_at(8, 2).map(|l| l.url.as_str()), Some("/x"));
        assert!(state.link_at(5, 2).is_none());
        assert!(state.link_at(8, 3).is_none());
    }

    #[test]
    fn anchors() {
        let mut state = MarkdownState::new("a\n\nb\n\n## The End\n\nc");
        state.scroll_to_anchor("the-end");
        draw(&mut state, 20, 2);
        assert_eq!(state.scroll(), 4);
    }
}
