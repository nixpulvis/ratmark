# ratmark

Render Markdown into [Ratatui] text.

```rust
let theme = ratmark::Theme::default();
let mut state = ratmark::MarkdownState::new(markdown);

// Each frame:
frame.render_stateful_widget(ratmark::MarkdownView::new(&theme), area, &mut state);

// On input:
state.scroll_by(1);
state.focus_next();
if let Some(link) = state.focused() { open(&link.url) }
if let Some(link) = state.link_at(mouse.column, mouse.row) { open(&link.url) }
```

`MarkdownView` lays the document out to fit, again whenever the width
changes, and draws it scrolled with the focused link highlighted. Ratatui
cells can't carry a URL, so the state keeps track of where every link landed,
which is what makes keyboard focus and clicks work on links in the middle of
wrapped text. For drawing a document some other way, `ratmark::render` returns
the wrapped lines and link positions directly.

It handles CommonMark plus tables, footnotes, strikethrough, task lists and
math. Code blocks are syntax highlighted with [syntect], through
the default `highlight` feature. That uses syntect's pure Rust regex engine,
so ratmark also builds for `wasm32-unknown-unknown` and works in the browser
with something like [Ratzilla].

syntect's bundled syntaxes and themes come with the `default-syntaxes`
feature, also on by default, and add a megabyte or two. To keep a build
small, turn it off and pass `highlight::set` only the syntaxes you need.

Markdown written for real sites tends to have HTML mixed in, so ratmark reads
the text and links out of it instead of printing tags.

Two optional features help with Markdown from static sites. `front-matter`
splits YAML or TOML front matter off the top of a document and has a small
helper for reading values like the title.

The `kramdown` feature understands a couple of kramdown habits, since
that's what Jekyll uses. Attribute lists like `{:.no_toc}` are dropped, and
Markdown inside `markdown="1"` elements is rendered as Markdown.

Two themes come with it. `Theme::gruvbox()` is the default, and
`Theme::plain()` uses no colors so the terminal's own palette shows through.
Every style is a public field, so making your own is a struct literal.

## Credit

The idea and the look come from Tyler Bloom's blog, [The Avid
Rustacean][avid-rustacean], which renders its posts with Ratatui in the
browser. ratmark is a fresh implementation, not a copy of that code.

## Testing

`cargo test` runs the unit tests. To check a pile of real documents, point
`RATMARK_CORPUS` at a directory of Markdown files. Every file is rendered at
several widths, checking that nothing panics and no line is too wide.

```sh
RATMARK_CORPUS=~/site/_site cargo test --features kramdown corpus
```

`cargo run --example dump -- FILE WIDTH` prints a file as rendered, without
colors, which is handy for eyeballing layout.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option.

[Ratatui]: https://ratatui.rs
[syntect]: https://github.com/trishume/syntect
[Ratzilla]: https://github.com/orhun/ratzilla
[avid-rustacean]: https://github.com/TylerBloom/avid-rustacean
