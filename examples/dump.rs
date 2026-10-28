//! Prints a Markdown file as rendered text, without colors, to check layout.
//!
//!     cargo run --example dump -- README.md 60

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().expect("usage: dump FILE [WIDTH]");
    let width = args
        .next()
        .map_or(80, |w| w.parse().expect("width is a number"));
    let markdown = std::fs::read_to_string(path).expect("readable file");
    let rendered = ratmark::render(&markdown, width, &ratmark::Theme::default());
    for line in &rendered.lines {
        let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        println!("{}", text.trim_end());
    }
    for link in &rendered.links {
        eprintln!("{} {:?}", link.url, link.regions);
    }
}
