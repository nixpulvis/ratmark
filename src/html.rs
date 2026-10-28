//! Just enough HTML to read the text and links out of the HTML embedded in
//! Markdown. Nothing here builds a tree or tries to be a real parser.

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Token {
    Text(String),
    Open {
        name: String,
        attrs: Vec<(String, String)>,
    },
    Close(String),
}

impl Token {
    pub fn attr(&self, key: &str) -> Option<&str> {
        match self {
            Token::Open { attrs, .. } => attrs
                .iter()
                .find(|(k, _)| k == key)
                .map(|(_, v)| v.as_str()),
            _ => None,
        }
    }
}

/// Elements whose content is never shown as text.
const SKIPPED: &[&str] = &["script", "style", "svg", "template", "head"];

/// Splits HTML into text and tags. Comments are dropped, along with the
/// content of [`SKIPPED`] elements, though their opening tag is still
/// returned so its attributes can be used.
pub(crate) fn tokenize(html: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut rest = html;
    while !rest.is_empty() {
        if let Some(after) = rest.strip_prefix("<!--") {
            rest = after.find("-->").map_or("", |i| &after[i + 3..]);
            continue;
        }
        if rest.starts_with('<')
            && let Some(end) = rest.find('>')
            && let Some(token) = tag(&rest[1..end])
        {
            rest = &rest[end + 1..];
            if let Token::Open { name, .. } = &token
                && SKIPPED.contains(&name.as_str())
            {
                let close = format!("</{name}");
                let found = rest.to_ascii_lowercase().find(&close);
                rest = found.map_or("", |i| {
                    let after = &rest[i..];
                    after.find('>').map_or("", |j| &after[j + 1..])
                });
            }
            tokens.push(token);
            continue;
        }
        // Text runs up to the next thing that could be a tag.
        let first = rest.chars().next().map_or(0, char::len_utf8);
        let next = rest[first..].find('<').map_or(rest.len(), |i| i + first);
        tokens.push(Token::Text(decode(&rest[..next])));
        rest = &rest[next..];
    }
    tokens
}

/// Parses the inside of `<...>`, or returns `None` if it isn't a tag.
fn tag(inner: &str) -> Option<Token> {
    if let Some(name) = inner.strip_prefix('/') {
        let name = name.trim().to_ascii_lowercase();
        return valid(&name).then_some(Token::Close(name));
    }
    let inner = inner.trim_end_matches('/');
    let name_end = inner
        .find(|c: char| c.is_whitespace())
        .unwrap_or(inner.len());
    let name = inner[..name_end].to_ascii_lowercase();
    if !valid(&name) {
        return None;
    }
    let mut attrs = Vec::new();
    let mut rest = inner[name_end..].trim_start();
    while !rest.is_empty() {
        let key_end = rest
            .find(|c: char| c == '=' || c.is_whitespace())
            .unwrap_or(rest.len());
        let key = rest[..key_end].to_ascii_lowercase();
        rest = rest[key_end..].trim_start();
        let mut value = String::new();
        if let Some(after) = rest.strip_prefix('=') {
            let after = after.trim_start();
            let (v, r) = match after.chars().next() {
                Some(q @ ('"' | '\'')) => {
                    let body = &after[1..];
                    let end = body.find(q).unwrap_or(body.len());
                    (&body[..end], body.get(end + 1..).unwrap_or(""))
                }
                _ => {
                    let end = after.find(char::is_whitespace).unwrap_or(after.len());
                    (&after[..end], &after[end..])
                }
            };
            value = decode(v);
            rest = r.trim_start();
        }
        if !key.is_empty() {
            attrs.push((key, value));
        }
    }
    Some(Token::Open { name, attrs })
}

fn valid(name: &str) -> bool {
    name.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

/// Decodes the character references that show up in hand-written HTML.
pub(crate) fn decode(text: &str) -> String {
    if !text.contains('&') {
        return text.to_owned();
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        let end = rest.find(';').filter(|&e| e <= 10);
        let decoded = end.and_then(|e| entity(&rest[1..e]));
        match (end, decoded) {
            (Some(e), Some(c)) => {
                out.push(c);
                rest = &rest[e + 1..];
            }
            _ => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

fn entity(name: &str) -> Option<char> {
    if let Some(num) = name.strip_prefix('#') {
        let code = match num.strip_prefix(['x', 'X']) {
            Some(hex) => u32::from_str_radix(hex, 16).ok()?,
            None => num.parse().ok()?,
        };
        return char::from_u32(code);
    }
    Some(match name {
        "amp" => '&',
        "lt" => '<',
        "gt" => '>',
        "quot" => '"',
        "apos" => '\'',
        "nbsp" => '\u{a0}',
        "ndash" => '–',
        "mdash" => '—',
        "middot" => '·',
        "hellip" => '…',
        "copy" => '©',
        "rarr" => '→',
        "larr" => '←',
        "lsquo" => '‘',
        "rsquo" => '’',
        "ldquo" => '“',
        "rdquo" => '”',
        "times" => '×',
        "deg" => '°',
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn links_and_text() {
        let tokens = tokenize(r#"<li><a href="/x?a=1&amp;b=2">Hi &amp; bye</a></li>"#);
        assert_eq!(tokens[1].attr("href"), Some("/x?a=1&b=2"));
        assert_eq!(tokens[2], Token::Text("Hi & bye".into()));
        assert_eq!(tokens[3], Token::Close("a".into()));
    }

    #[test]
    fn skips_svg_content_and_comments() {
        let tokens = tokenize(r#"a<!-- no --><svg aria-label="fig"><text>x</text></svg>b"#);
        assert_eq!(tokens.len(), 3);
        assert_eq!(tokens[1].attr("aria-label"), Some("fig"));
        assert_eq!(tokens[2], Token::Text("b".into()));
    }

    #[test]
    fn multibyte_text() {
        assert_eq!(
            tokenize("λ<b>"),
            [
                Token::Text("λ".into()),
                Token::Open {
                    name: "b".into(),
                    attrs: vec![]
                }
            ]
        );
    }

    #[test]
    fn stray_angle_brackets_are_text() {
        let tokens = tokenize("1 < 2 > 0");
        let text: String = tokens
            .iter()
            .map(|t| match t {
                Token::Text(s) => s.as_str(),
                _ => "",
            })
            .collect();
        assert_eq!(text, "1 < 2 > 0");
    }
}
