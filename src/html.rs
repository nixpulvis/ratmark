//! The text and tags of the HTML embedded in Markdown, from html5gum's
//! tokenizer. Nothing here builds a tree.

use html5gum::{DefaultEmitter, Token as Html, Tokenizer};

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
    let mut emitter = DefaultEmitter::default();
    // Read the insides of elements like `<script>` as text, like a browser.
    emitter.naively_switch_states(true);
    let mut tokens: Vec<Token> = Vec::new();
    let mut skipping: Option<String> = None;
    for token in Tokenizer::new_with_emitter(html, emitter).flatten() {
        match token {
            Html::StartTag(tag) if skipping.is_none() => {
                let name = string(&tag.name);
                if SKIPPED.contains(&name.as_str()) && !tag.self_closing {
                    skipping = Some(name.clone());
                }
                let attrs = tag
                    .attributes
                    .iter()
                    .map(|(k, v)| (string(k), string(v)))
                    .collect();
                tokens.push(Token::Open { name, attrs });
            }
            Html::EndTag(tag) => {
                let name = string(&tag.name);
                if skipping.is_none() {
                    tokens.push(Token::Close(name));
                } else if skipping.as_ref() == Some(&name) {
                    skipping = None;
                }
            }
            Html::String(text) if skipping.is_none() => {
                // The tokenizer can hand text over in pieces.
                if let Some(Token::Text(last)) = tokens.last_mut() {
                    last.push_str(&string(&text));
                } else {
                    tokens.push(Token::Text(string(&text)));
                }
            }
            _ => {}
        }
    }
    tokens
}

fn string(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
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
