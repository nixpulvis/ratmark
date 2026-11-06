//! Highlights fenced code by its language, with the `syntax` feature.
//!
//!     cargo run --example syntax --features syntax

mod common;

use ratmark::MarkdownState;

const MARKDOWN: &str = r#"Fenced code is colored by its language:

```rust
fn main() {
    let name = "world";
    println!("Hello, {name}!"); // A comment.
}
```

```sh
cargo run --example syntax --features syntax
```

An unknown language, or none, stays plain:

```
just text
```
"#;

fn main() -> std::io::Result<()> {
    // The theme's "ansi" syntax colors use the terminal's own palette.
    // Any of syntect's bundled themes, like "base16-ocean.dark", can be named
    // in `Theme::syntax` instead. Only their foreground colors are used, over
    // the theme's code block background.
    common::print(&mut MarkdownState::new(MARKDOWN), &common::theme())
}
