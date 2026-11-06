//! Renders every Markdown file under the directories it's given at several
//! widths, checking that nothing panics and no line is too wide.
//!
//!     cargo test --features kramdown --test corpus -- ~/site
//!
//! `cargo test` runs it too, with no directory, so it skips. Arguments that
//! aren't directories, like the filters `cargo test` passes every test
//! binary, are ignored.

mod common;

use std::panic;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use unicode_width::UnicodeWidthStr;

const WIDTHS: [u16; 6] = [1, 8, 20, 45, 80, 200];

fn main() -> ExitCode {
    let (dirs, ignored): (Vec<PathBuf>, Vec<PathBuf>) = std::env::args()
        .skip(1)
        .map(PathBuf::from)
        .partition(|path| path.is_dir());
    if dirs.is_empty() {
        println!("corpus: skipped, pass it a directory: cargo test --test corpus -- DIR");
        if !ignored.is_empty() {
            println!("corpus: not directories: {ignored:?}");
        }
        return ExitCode::SUCCESS;
    }

    let mut files = Vec::new();
    for dir in &dirs {
        find_markdown(dir, &mut files);
    }
    if files.is_empty() {
        println!("corpus: no .md files under {dirs:?}");
        return ExitCode::FAILURE;
    }

    // Report panics as failures of their file, without the default message.
    panic::set_hook(Box::new(|_| {}));
    let mut failures = 0;
    for path in &files {
        let markdown = std::fs::read_to_string(path).expect("readable file");
        for width in WIDTHS {
            let Ok(lines) = panic::catch_unwind(|| common::lines(&markdown, width)) else {
                println!("{} panics at width {width}", path.display());
                failures += 1;
                continue;
            };
            for (i, line) in lines.iter().enumerate() {
                if line.width() > usize::from(width) {
                    println!(
                        "{}:{i} is {} wide at {width}: {line:?}",
                        path.display(),
                        line.width(),
                    );
                    failures += 1;
                }
            }
        }
    }

    println!("corpus: {} files, {failures} failures", files.len());
    if failures == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn find_markdown(path: &Path, files: &mut Vec<PathBuf>) {
    if path.is_dir() {
        for entry in std::fs::read_dir(path).expect("readable directory") {
            find_markdown(&entry.expect("readable entry").path(), files);
        }
    } else if path.extension().is_some_and(|e| e == "md") {
        files.push(path.to_owned());
    }
}
