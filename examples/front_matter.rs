//! Splits front matter off the top of a document, with the `front-matter`
//! feature, rather than rendering it as text.
//!
//!     cargo run --example front_matter --features front-matter

mod common;

use ratmark::front_matter::FrontMatter;
use ratmark::{MarkdownState, Theme};

const MARKDOWN: &str = "---
title: 'Notes: week one'
date: 2026-10-31
tags: [rust, tui]
---

# Week one

The front matter above isn't drawn.
";

fn main() {
    let (front, body) = FrontMatter::split(MARKDOWN);
    let front = front.expect("the document has front matter");
    println!("format: {:?}", front.format);
    println!("title:  {:?}", front.get("title"));
    println!("date:   {:?}", front.get("date"));
    // Lists and tables need a real YAML or TOML parser, on `front.raw`.
    println!("tags:   {:?}", front.get("tags"));
    println!("body:   {} bytes\n", body.len());

    // Rendering the whole document leaves the front matter out too.
    common::print(&mut MarkdownState::new(MARKDOWN), &Theme::default());
}
