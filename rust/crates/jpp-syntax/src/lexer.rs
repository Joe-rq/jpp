use crate::{ast::Span, diagnostic::Diagnostic};

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Kind {
    Name(String),
    Number(String),
    Text(String),
    Symbol(String),
    End,
}

#[derive(Clone, Debug)]
pub(crate) struct Token {
    pub kind: Kind,
    pub span: Span,
}

pub(crate) fn lex(source: &str) -> Result<Vec<Token>, Diagnostic> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < source.len() {
        let c = source[i..].chars().next().unwrap();
        if c.is_whitespace() {
            i += c.len_utf8();
            continue;
        }
        if source[i..].starts_with("//") {
            i += source[i..].find('\n').unwrap_or(source.len() - i);
            continue;
        }
        let start = i;
        let kind = if c == '"' {
            i += 1;
            let mut value = String::new();
            let mut closed = false;
            while i < source.len() {
                let ch = source[i..].chars().next().unwrap();
                i += ch.len_utf8();
                if ch == '"' {
                    closed = true;
                    break;
                }
                if ch == '\\' {
                    let Some(escaped) = source[i..].chars().next() else {
                        break;
                    };
                    i += escaped.len_utf8();
                    value.push(match escaped {
                        'n' => '\n',
                        'r' => '\r',
                        't' => '\t',
                        '"' => '"',
                        '\\' => '\\',
                        _ => {
                            return Err(Diagnostic::new(
                                "unknown string escape",
                                Span {
                                    start: i - escaped.len_utf8() - 1,
                                    end: i,
                                },
                            ));
                        }
                    });
                } else {
                    value.push(ch);
                }
            }
            if !closed {
                return Err(Diagnostic::new(
                    "unterminated string",
                    Span { start, end: i },
                ));
            }
            Kind::Text(value)
        } else if c.is_ascii_digit() {
            i += 1;
            while i < source.len() && source.as_bytes()[i].is_ascii_digit() {
                i += 1;
            }
            if source.as_bytes().get(i) == Some(&b'.')
                && source.as_bytes().get(i + 1).is_some_and(u8::is_ascii_digit)
            {
                i += 1;
                while i < source.len() && source.as_bytes()[i].is_ascii_digit() {
                    i += 1;
                }
            }
            Kind::Number(source[start..i].to_owned())
        } else if c == '_' || c.is_alphabetic() {
            i += c.len_utf8();
            while let Some(ch) = source[i..].chars().next() {
                if ch == '_' || ch.is_alphanumeric() {
                    i += ch.len_utf8();
                } else {
                    break;
                }
            }
            Kind::Name(source[start..i].to_owned())
        } else {
            let pair = ["->", "==", "!=", "<=", ">=", "&&", "||"]
                .into_iter()
                .find(|p| source[i..].starts_with(p));
            if let Some(pair) = pair {
                i += 2;
                Kind::Symbol(pair.to_owned())
            } else if "(){}[],:;.+-*/%!=<>".contains(c) {
                i += c.len_utf8();
                Kind::Symbol(c.to_string())
            } else {
                return Err(Diagnostic::new(
                    format!("unexpected character {c:?}"),
                    Span {
                        start,
                        end: i + c.len_utf8(),
                    },
                ));
            }
        };
        out.push(Token {
            kind,
            span: Span { start, end: i },
        });
    }
    out.push(Token {
        kind: Kind::End,
        span: Span { start: i, end: i },
    });
    Ok(out)
}
