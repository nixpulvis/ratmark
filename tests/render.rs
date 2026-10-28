use ratmark::{Region, Theme, render};
use unicode_width::UnicodeWidthStr;

fn lines(markdown: &str, width: u16) -> Vec<String> {
    render(markdown, width, &Theme::default())
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

#[test]
fn paragraphs_wrap_with_gaps() {
    assert_eq!(
        lines("one two three\n\nfour", 8),
        ["one two", "three", "", "four"]
    );
}

#[test]
fn headings_keep_their_marks() {
    assert_eq!(lines("## Hi there", 40), ["## Hi there"]);
}

#[test]
fn nested_lists_indent() {
    let md = "- a\n- b long words\n  - c\n1. d\n2. e";
    assert_eq!(
        lines(md, 10),
        ["• a", "• b long", "  words", "  ◦ c", "", "1. d", "2. e"]
    );
}

#[test]
fn quotes_keep_their_bar_when_wrapped() {
    assert_eq!(lines("> one two three", 9), ["▎ one two", "▎ three"]);
}

#[test]
fn quotes_start_after_a_plain_gap() {
    assert_eq!(lines("a\n\n> b\n>\n> c", 20), ["a", "", "▎ b", "▎", "▎ c"]);
}

#[test]
fn code_blocks_fill_the_width() {
    let rendered = render("```\nfn x\n```", 12, &Theme::default());
    let line = &rendered.lines[0];
    assert_eq!(line.width(), 12);
    let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
    assert_eq!(text.trim_end(), " fn x");
}

#[test]
fn long_code_lines_wrap_inside_the_block() {
    let out = lines("```\nabcdefgh\n```", 6);
    assert_eq!(out, [" abcd", " efgh"]);
}

#[test]
fn links_record_where_they_land() {
    let rendered = render("go [over there](/x) now", 10, &Theme::default());
    let link = &rendered.links[0];
    assert_eq!(link.url, "/x");
    assert_eq!(
        link.regions,
        [
            Region {
                line: 0,
                start: 3,
                end: 7
            },
            Region {
                line: 1,
                start: 0,
                end: 5
            }
        ]
    );
    assert_eq!(rendered.link_at(1, 2).map(|l| l.url.as_str()), Some("/x"));
    assert_eq!(rendered.link_at(0, 0), None);
}

#[test]
fn html_links_and_lists() {
    let md = "<ul>\n<li><a href=\"/a\">A</a> <small>1 word</small></li>\n<li>B</li>\n</ul>";
    let rendered = render(md, 40, &Theme::default());
    assert_eq!(rendered.links[0].url, "/a");
    assert_eq!(lines(md, 40), ["• A 1 word", "• B"]);
}

#[test]
fn tables_line_up() {
    let md = "| a | bb |\n|---|---:|\n| ccc | d |";
    assert_eq!(lines(md, 40), ["a   │ bb", "────┼───", "ccc │  d"]);
}

#[cfg(feature = "front-matter")]
#[test]
fn front_matter_is_kept_aside() {
    let rendered = render("---\ntitle: Hi\n---\n\nBody", 40, &Theme::default());
    let front = rendered.front_matter.unwrap();
    assert_eq!(front.get("title").as_deref(), Some("Hi"));
    assert_eq!(rendered.lines.len(), 1);
}

#[cfg(feature = "kramdown")]
#[test]
fn kramdown_attribute_lists_are_dropped() {
    assert_eq!(
        lines("## Notes\n{:.no_toc}\n\ntext", 40),
        ["## Notes", "", "text"]
    );
}

#[test]
fn images_become_links() {
    let rendered = render("![a cat](/cat.png)", 40, &Theme::default());
    assert_eq!(rendered.links[0].url, "/cat.png");
    assert_eq!(lines("![a cat](/cat.png)", 40), ["[image: a cat]"]);
}

/// Renders every Markdown file under `$RATMARK_CORPUS` at several widths,
/// checking nothing panics and no line is too wide.
#[test]
fn corpus() {
    let Ok(dir) = std::env::var("RATMARK_CORPUS") else {
        return;
    };
    let mut stack = vec![std::path::PathBuf::from(dir)];
    let mut count = 0;
    while let Some(path) = stack.pop() {
        if path.is_dir() {
            stack.extend(std::fs::read_dir(&path).unwrap().map(|e| e.unwrap().path()));
            continue;
        }
        if path.extension().is_none_or(|e| e != "md") {
            continue;
        }
        let markdown = std::fs::read_to_string(&path).unwrap();
        for width in [1, 8, 20, 45, 80, 200] {
            for (i, line) in lines(&markdown, width).iter().enumerate() {
                assert!(
                    line.width() <= usize::from(width),
                    "{}:{i} is {} wide at {width}: {line:?}",
                    path.display(),
                    line.width(),
                );
            }
        }
        count += 1;
    }
    assert!(count > 0, "no Markdown found");
}

#[test]
fn headings_are_listed() {
    let rendered = render("intro\n\n## One *two*\n\ntext", 40, &Theme::default());
    let heading = &rendered.headings[0];
    assert_eq!(
        (heading.level, heading.text.as_str(), heading.line),
        (2, "One two", 2)
    );
    let text: String = rendered.lines[2]
        .spans
        .iter()
        .map(|s| s.content.as_ref())
        .collect();
    assert_eq!(text, "## One two");
}

#[test]
fn small_print_in_a_heading_is_a_subtitle() {
    let md = "<h2>\n    2026\n    <small><a href=\"/x\">text</a></small>\n</h2>\n\nafter";
    assert_eq!(lines(md, 40), ["2026", "text", "", "after"]);
}

#[test]
fn html_items_hang_their_wrapped_lines() {
    let md = "<ul><li>one two three</li><li>a<br>b</li></ul>\n\nafter";
    assert_eq!(
        lines(md, 9),
        ["• one two", "  three", "• a", "  b", "", "after"]
    );
}

#[test]
fn html_headings_after_html_lists_get_a_gap() {
    let md = "<ul><li>a</li></ul><h2>B</h2>";
    assert_eq!(lines(md, 9), ["• a", "", "B"]);
}
