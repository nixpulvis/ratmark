//! Front matter, the block of metadata some Markdown starts with.
//!
//! ```
//! use ratmark::front_matter::FrontMatter;
//!
//! let (front, body) = FrontMatter::split("---\ntitle: 'Hi: there'\n---\n\n# Hi");
//! assert_eq!(front.unwrap().get("title").as_deref(), Some("Hi: there"));
//! assert_eq!(body, "\n# Hi");
//! ```

/// Front matter fenced by `---` lines, as YAML, or by `+++` lines, as TOML.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrontMatter {
    pub format: Format,
    /// The text between the fences, unparsed.
    pub raw: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Yaml,
    Toml,
}

impl Format {
    /// The line above and below the front matter.
    fn fence(self) -> &'static str {
        match self {
            Format::Yaml => "---",
            Format::Toml => "+++",
        }
    }

    /// What goes between a key and its value.
    fn separator(self) -> char {
        match self {
            Format::Yaml => ':',
            Format::Toml => '=',
        }
    }
}

impl FrontMatter {
    /// Splits front matter off the start of `markdown`, returning it and the
    /// rest of the document.
    pub fn split(markdown: &str) -> (Option<Self>, &str) {
        for format in [Format::Yaml, Format::Toml] {
            let fence = format.fence();
            let Some(rest) = markdown.strip_prefix(fence) else {
                continue;
            };
            let Some(rest) = rest
                .strip_prefix('\n')
                .or_else(|| rest.strip_prefix("\r\n"))
            else {
                continue;
            };
            let mut offset = 0;
            for line in rest.split_inclusive('\n') {
                if line.trim_end() == fence {
                    let raw = rest[..offset].to_owned();
                    return (Some(Self { format, raw }), &rest[offset + line.len()..]);
                }
                offset += line.len();
            }
        }
        (None, markdown)
    }

    /// A top level string, number or date value, like `title`. This is no
    /// YAML or TOML parser, so lists, tables and multi-line strings come back
    /// as `None`. Reach for a real parser on [`FrontMatter::raw`] for those.
    pub fn get(&self, key: &str) -> Option<String> {
        let separator = self.format.separator();
        let value = self.raw.lines().find_map(|line| {
            let (k, v) = line.split_once(separator)?;
            (k.trim() == key && !line.starts_with(char::is_whitespace)).then_some(v.trim())
        })?;
        if value.is_empty() || value.starts_with(['[', '{', '|', '>']) {
            return None;
        }
        if let Some(s) = value.strip_prefix('\'').and_then(|s| s.strip_suffix('\'')) {
            return Some(match self.format {
                Format::Yaml => s.replace("''", "'"),
                Format::Toml => s.to_owned(),
            });
        }
        if let Some(s) = value.strip_prefix('"').and_then(|s| s.strip_suffix('"')) {
            return Some(s.replace("\\\"", "\"").replace("\\\\", "\\"));
        }
        Some(value.to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn yaml() {
        let (front, body) = FrontMatter::split(
            "---\ntitle: \"Say \\\"hi\\\"\"\ndate: 2026-10-28\ntags:\n  - a\n---\nbody",
        );
        let front = front.unwrap();
        assert_eq!(front.format, Format::Yaml);
        assert_eq!(front.get("title").as_deref(), Some("Say \"hi\""));
        assert_eq!(front.get("date").as_deref(), Some("2026-10-28"));
        assert_eq!(front.get("tags"), None);
        assert_eq!(front.get("missing"), None);
        assert_eq!(body, "body");
    }

    #[test]
    fn toml() {
        let (front, body) = FrontMatter::split("+++\ntitle = 'Hi'\n+++\n");
        assert_eq!(front.unwrap().get("title").as_deref(), Some("Hi"));
        assert_eq!(body, "");
    }

    #[test]
    fn none() {
        assert_eq!(
            FrontMatter::split("--- not front matter"),
            (None, "--- not front matter")
        );
        assert_eq!(
            FrontMatter::split("---\nnever closed"),
            (None, "---\nnever closed")
        );
        assert_eq!(FrontMatter::split("# Hi"), (None, "# Hi"));
    }
}
