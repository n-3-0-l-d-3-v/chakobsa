//! CHAKOBSA's lexer: turns source text into a flat token stream. Nothing
//! downstream (the parser, ticket 003) builds a separate lossless token
//! tree or re-scans the source — this is the one and only tokenization
//! pass, per `docs/design/CONSTRAINTS.md`'s "justify every conventional
//! structure it keeps."
//!
//! Malformed input is always a `LexError`, never a panic — the same
//! contract every other subsystem in this project holds itself to for
//! untrusted/corrupt input (see e.g. sietch's `Segment::recover` and
//! mentat's `DecodeError`).

use std::fmt;

/// A byte-offset range into the source text, `[start, end)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenKind {
    // Literals and identifiers.
    Ident(String),
    Int(i64),
    True,
    False,

    // Keywords.
    Let,
    Fn,
    If,
    Else,
    While,
    Return,
    And,
    Or,
    Not,
    I64,
    Bool,

    // Operators.
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    EqEq,
    NotEq,
    Lt,
    Le,
    Gt,
    Ge,
    Eq,

    // Punctuation.
    LParen,
    RParen,
    LBrace,
    RBrace,
    Comma,
    Semicolon,
    Colon,
    Arrow,

    Eof,
}

impl fmt::Display for TokenKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use TokenKind::*;
        match self {
            Ident(s) => write!(f, "identifier '{s}'"),
            Int(n) => write!(f, "integer {n}"),
            True => write!(f, "'true'"),
            False => write!(f, "'false'"),
            Let => write!(f, "'let'"),
            Fn => write!(f, "'fn'"),
            If => write!(f, "'if'"),
            Else => write!(f, "'else'"),
            While => write!(f, "'while'"),
            Return => write!(f, "'return'"),
            And => write!(f, "'and'"),
            Or => write!(f, "'or'"),
            Not => write!(f, "'not'"),
            I64 => write!(f, "'i64'"),
            Bool => write!(f, "'bool'"),
            Plus => write!(f, "'+'"),
            Minus => write!(f, "'-'"),
            Star => write!(f, "'*'"),
            Slash => write!(f, "'/'"),
            Percent => write!(f, "'%'"),
            EqEq => write!(f, "'=='"),
            NotEq => write!(f, "'!='"),
            Lt => write!(f, "'<'"),
            Le => write!(f, "'<='"),
            Gt => write!(f, "'>'"),
            Ge => write!(f, "'>='"),
            Eq => write!(f, "'='"),
            LParen => write!(f, "'('"),
            RParen => write!(f, "')'"),
            LBrace => write!(f, "'{{'"),
            RBrace => write!(f, "'}}'"),
            Comma => write!(f, "','"),
            Semicolon => write!(f, "';'"),
            Colon => write!(f, "':'"),
            Arrow => write!(f, "'->'"),
            Eof => write!(f, "end of input"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum LexError {
    #[error("unexpected character '{ch}' at byte offset {pos}")]
    UnexpectedChar { ch: char, pos: usize },
    #[error("integer literal at byte offset {pos} does not fit in a 64-bit signed integer")]
    IntegerOverflow { pos: usize },
    #[error("unterminated '!' at byte offset {pos} (did you mean '!='?)")]
    LoneBang { pos: usize },
}

/// True if `ident` would lex as a keyword rather than an identifier —
/// exposed so callers building source text (tests, and later the
/// parser's error messages) can check without duplicating the keyword
/// table.
pub fn is_reserved_word(ident: &str) -> bool {
    keyword(ident).is_some()
}

fn keyword(ident: &str) -> Option<TokenKind> {
    use TokenKind::*;
    Some(match ident {
        "let" => Let,
        "fn" => Fn,
        "if" => If,
        "else" => Else,
        "while" => While,
        "return" => Return,
        "and" => And,
        "or" => Or,
        "not" => Not,
        "i64" => I64,
        "bool" => Bool,
        "true" => True,
        "false" => False,
        _ => return None,
    })
}

/// Tokenizes `source` in full, returning every token including a trailing
/// `Eof`, or the first lexical error encountered. There is no partial/
/// recovery mode — a single bad token fails the whole lex, matching this
/// project's "malformed input is a typed error, not best-effort" stance.
pub fn lex(source: &str) -> Result<Vec<Token>, LexError> {
    let bytes = source.as_bytes();
    let mut tokens = Vec::new();
    let mut pos = 0usize;

    while pos < bytes.len() {
        let ch = bytes[pos] as char;

        if ch.is_whitespace() {
            pos += 1;
            continue;
        }

        if ch == '/' && bytes.get(pos + 1) == Some(&b'/') {
            while pos < bytes.len() && bytes[pos] != b'\n' {
                pos += 1;
            }
            continue;
        }

        if ch.is_ascii_digit() {
            let start = pos;
            while pos < bytes.len() && (bytes[pos] as char).is_ascii_digit() {
                pos += 1;
            }
            let text = &source[start..pos];
            let value: i64 = text
                .parse()
                .map_err(|_| LexError::IntegerOverflow { pos: start })?;
            tokens.push(Token {
                kind: TokenKind::Int(value),
                span: Span::new(start, pos),
            });
            continue;
        }

        if ch.is_ascii_alphabetic() || ch == '_' {
            let start = pos;
            while pos < bytes.len() {
                let c = bytes[pos] as char;
                if c.is_ascii_alphanumeric() || c == '_' {
                    pos += 1;
                } else {
                    break;
                }
            }
            let text = &source[start..pos];
            let kind = keyword(text).unwrap_or_else(|| TokenKind::Ident(text.to_string()));
            tokens.push(Token {
                kind,
                span: Span::new(start, pos),
            });
            continue;
        }

        let start = pos;
        let (kind, len) = match ch {
            '+' => (TokenKind::Plus, 1),
            '-' => {
                if bytes.get(pos + 1) == Some(&b'>') {
                    (TokenKind::Arrow, 2)
                } else {
                    (TokenKind::Minus, 1)
                }
            }
            '*' => (TokenKind::Star, 1),
            '/' => (TokenKind::Slash, 1),
            '%' => (TokenKind::Percent, 1),
            '=' => {
                if bytes.get(pos + 1) == Some(&b'=') {
                    (TokenKind::EqEq, 2)
                } else {
                    (TokenKind::Eq, 1)
                }
            }
            '!' => {
                if bytes.get(pos + 1) == Some(&b'=') {
                    (TokenKind::NotEq, 2)
                } else {
                    return Err(LexError::LoneBang { pos });
                }
            }
            '<' => {
                if bytes.get(pos + 1) == Some(&b'=') {
                    (TokenKind::Le, 2)
                } else {
                    (TokenKind::Lt, 1)
                }
            }
            '>' => {
                if bytes.get(pos + 1) == Some(&b'=') {
                    (TokenKind::Ge, 2)
                } else {
                    (TokenKind::Gt, 1)
                }
            }
            '(' => (TokenKind::LParen, 1),
            ')' => (TokenKind::RParen, 1),
            '{' => (TokenKind::LBrace, 1),
            '}' => (TokenKind::RBrace, 1),
            ',' => (TokenKind::Comma, 1),
            ';' => (TokenKind::Semicolon, 1),
            ':' => (TokenKind::Colon, 1),
            other => return Err(LexError::UnexpectedChar { ch: other, pos }),
        };
        tokens.push(Token {
            kind,
            span: Span::new(start, start + len),
        });
        pos += len;
    }

    tokens.push(Token {
        kind: TokenKind::Eof,
        span: Span::new(bytes.len(), bytes.len()),
    });
    Ok(tokens)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(source: &str) -> Vec<TokenKind> {
        lex(source).unwrap().into_iter().map(|t| t.kind).collect()
    }

    #[test]
    fn empty_input_is_just_eof() {
        assert_eq!(kinds(""), vec![TokenKind::Eof]);
    }

    #[test]
    fn whitespace_and_comments_are_skipped() {
        assert_eq!(
            kinds("  \n\t// a comment\n  42 // trailing\n"),
            vec![TokenKind::Int(42), TokenKind::Eof]
        );
    }

    #[test]
    fn keywords_are_recognized_not_as_identifiers() {
        assert_eq!(
            kinds("let fn if else while return and or not i64 bool true false"),
            vec![
                TokenKind::Let,
                TokenKind::Fn,
                TokenKind::If,
                TokenKind::Else,
                TokenKind::While,
                TokenKind::Return,
                TokenKind::And,
                TokenKind::Or,
                TokenKind::Not,
                TokenKind::I64,
                TokenKind::Bool,
                TokenKind::True,
                TokenKind::False,
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn identifiers_can_contain_digits_and_underscores_but_not_start_with_a_digit() {
        assert_eq!(
            kinds("x x1 _foo foo_bar_2"),
            vec![
                TokenKind::Ident("x".into()),
                TokenKind::Ident("x1".into()),
                TokenKind::Ident("_foo".into()),
                TokenKind::Ident("foo_bar_2".into()),
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn a_keyword_prefixed_identifier_is_still_an_identifier() {
        // "iffy" must not be lexed as "if" + "fy".
        assert_eq!(
            kinds("iffy letter"),
            vec![
                TokenKind::Ident("iffy".into()),
                TokenKind::Ident("letter".into()),
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn two_character_operators_are_greedily_matched_over_one_character_ones() {
        assert_eq!(
            kinds("== != <= >= -> = < > + - * / %"),
            vec![
                TokenKind::EqEq,
                TokenKind::NotEq,
                TokenKind::Le,
                TokenKind::Ge,
                TokenKind::Arrow,
                TokenKind::Eq,
                TokenKind::Lt,
                TokenKind::Gt,
                TokenKind::Plus,
                TokenKind::Minus,
                TokenKind::Star,
                TokenKind::Slash,
                TokenKind::Percent,
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn punctuation_round_trips() {
        assert_eq!(
            kinds("( ) { } , ; :"),
            vec![
                TokenKind::LParen,
                TokenKind::RParen,
                TokenKind::LBrace,
                TokenKind::RBrace,
                TokenKind::Comma,
                TokenKind::Semicolon,
                TokenKind::Colon,
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn a_full_function_lexes_to_the_expected_token_sequence() {
        let src = "fn add(a: i64, b: i64) -> i64 { return a + b; }";
        assert_eq!(
            kinds(src),
            vec![
                TokenKind::Fn,
                TokenKind::Ident("add".into()),
                TokenKind::LParen,
                TokenKind::Ident("a".into()),
                TokenKind::Colon,
                TokenKind::I64,
                TokenKind::Comma,
                TokenKind::Ident("b".into()),
                TokenKind::Colon,
                TokenKind::I64,
                TokenKind::RParen,
                TokenKind::Arrow,
                TokenKind::I64,
                TokenKind::LBrace,
                TokenKind::Return,
                TokenKind::Ident("a".into()),
                TokenKind::Plus,
                TokenKind::Ident("b".into()),
                TokenKind::Semicolon,
                TokenKind::RBrace,
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn an_unexpected_character_is_a_typed_error_not_a_panic() {
        assert_eq!(lex("@"), Err(LexError::UnexpectedChar { ch: '@', pos: 0 }));
    }

    #[test]
    fn a_lone_bang_is_a_typed_error_but_bang_equals_is_fine() {
        assert_eq!(lex("!"), Err(LexError::LoneBang { pos: 0 }));
        assert!(lex("a != b").is_ok());
    }

    #[test]
    fn an_integer_literal_that_overflows_i64_is_a_typed_error() {
        assert_eq!(
            lex("99999999999999999999"),
            Err(LexError::IntegerOverflow { pos: 0 })
        );
    }

    #[test]
    fn integer_literal_at_exactly_i64_max_is_accepted() {
        assert_eq!(
            kinds(&i64::MAX.to_string()),
            vec![TokenKind::Int(i64::MAX), TokenKind::Eof]
        );
    }

    #[test]
    fn spans_point_at_the_correct_byte_offsets() {
        let tokens = lex("ab + 12").unwrap();
        assert_eq!(tokens[0].span, Span::new(0, 2)); // "ab"
        assert_eq!(tokens[1].span, Span::new(3, 4)); // "+"
        assert_eq!(tokens[2].span, Span::new(5, 7)); // "12"
    }
}
