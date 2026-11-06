//! What the integration tests share.

use ratmark::{Rendered, Theme};

/// `markdown` rendered `width` columns wide with the default theme, as plain
/// text without trailing spaces.
pub fn lines(markdown: &str, width: u16) -> Vec<String> {
    Rendered::render(markdown, width, &Theme::default())
        .lines
        .iter()
        .map(|l| {
            l.spans
                .iter()
                .map(|s| s.content.as_ref())
                .collect::<String>()
                .trim_end()
                .to_owned()
        })
        .collect()
}
