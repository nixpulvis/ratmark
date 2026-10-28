# ratmark

Render Markdown into [Ratatui] text.

```rust
let rendered = ratmark::render(markdown, area.width, &ratmark::Theme::default());
frame.render_widget(Paragraph::new(rendered.text()).scroll((offset, 0)), area);
```

`render` lays a document out for a given width and returns its lines, already
wrapped, along with where every link landed. A client can use those regions to
make links clickable or let the keyboard move between them.

It handles CommonMark plus tables, footnotes, strikethrough, task lists, math
and front matter. Code blocks are syntax highlighted with [syntect], through
the default `highlight` feature. That uses syntect's pure Rust regex engine,
so ratmark also builds for `wasm32-unknown-unknown` and works in the browser
with something like [Ratzilla].

Markdown written for real sites tends to have HTML mixed in, so ratmark reads
the text and links out of it instead of printing tags. It also understands a
couple of kramdown habits, since that's what Jekyll uses: attribute lists like
`{:.no_toc}` are dropped, and Markdown inside `markdown="1"` elements is
rendered as Markdown.

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
RATMARK_CORPUS=~/site/_site cargo test corpus
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
