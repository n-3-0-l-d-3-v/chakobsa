//! Property-based tests: the lexer must never panic on arbitrary input
//! (it always returns `Ok` or a typed `LexError`), and a token stream
//! built from known-good pieces must always re-lex to exactly the tokens
//! it was assembled from.

use lexer::{lex, Token, TokenKind};
use proptest::prelude::*;

proptest! {
    /// Completely arbitrary bytes (interpreted as a UTF-8-lossy string,
    /// since the lexer's input type is `&str`) must never panic — only
    /// ever `Ok` or a typed `LexError`, mirroring this project's
    /// "malformed input is a typed error" rule everywhere else.
    #[test]
    fn lex_never_panics_on_arbitrary_input(bytes in prop::collection::vec(any::<u8>(), 0..200)) {
        let source = String::from_utf8_lossy(&bytes);
        let _ = lex(&source); // must not panic, Ok or Err both fine
    }

    /// Arbitrary sequences of well-formed tokens, rendered back to source
    /// text with a single space between each, must re-lex to exactly the
    /// same sequence of token kinds they were built from.
    #[test]
    fn a_rendered_token_sequence_relexes_to_itself(
        kinds in prop::collection::vec(arb_simple_token_kind(), 1..30)
    ) {
        let rendered: Vec<String> = kinds.iter().map(render).collect();
        let source = rendered.join(" ");
        let tokens = lex(&source).unwrap();
        let relexed_kinds: Vec<TokenKind> = tokens
            .into_iter()
            .map(|t: Token| t.kind)
            .filter(|k| *k != TokenKind::Eof)
            .collect();
        prop_assert_eq!(relexed_kinds, kinds);
    }
}

fn arb_simple_token_kind() -> impl Strategy<Value = TokenKind> {
    prop_oneof![
        (0..1_000_000i64).prop_map(TokenKind::Int),
        "[a-z][a-z0-9_]{0,8}".prop_filter_map("must not collide with a keyword", |s| {
            if lexer::is_reserved_word(&s) {
                None
            } else {
                Some(TokenKind::Ident(s))
            }
        }),
        Just(TokenKind::Plus),
        Just(TokenKind::Minus),
        Just(TokenKind::Star),
        Just(TokenKind::Slash),
        Just(TokenKind::Percent),
        Just(TokenKind::EqEq),
        Just(TokenKind::NotEq),
        Just(TokenKind::Lt),
        Just(TokenKind::Le),
        Just(TokenKind::Gt),
        Just(TokenKind::Ge),
        Just(TokenKind::Eq),
        Just(TokenKind::LParen),
        Just(TokenKind::RParen),
        Just(TokenKind::LBrace),
        Just(TokenKind::RBrace),
        Just(TokenKind::Comma),
        Just(TokenKind::Semicolon),
        Just(TokenKind::Colon),
        Just(TokenKind::Arrow),
        Just(TokenKind::Let),
        Just(TokenKind::Fn),
        Just(TokenKind::If),
        Just(TokenKind::Else),
        Just(TokenKind::While),
        Just(TokenKind::Return),
        Just(TokenKind::And),
        Just(TokenKind::Or),
        Just(TokenKind::Not),
        Just(TokenKind::I64),
        Just(TokenKind::Bool),
        Just(TokenKind::True),
        Just(TokenKind::False),
    ]
}

fn render(kind: &TokenKind) -> String {
    use TokenKind::*;
    match kind {
        Ident(s) => s.clone(),
        Int(n) => n.to_string(),
        True => "true".into(),
        False => "false".into(),
        Let => "let".into(),
        Fn => "fn".into(),
        If => "if".into(),
        Else => "else".into(),
        While => "while".into(),
        Return => "return".into(),
        And => "and".into(),
        Or => "or".into(),
        Not => "not".into(),
        I64 => "i64".into(),
        Bool => "bool".into(),
        Plus => "+".into(),
        Minus => "-".into(),
        Star => "*".into(),
        Slash => "/".into(),
        Percent => "%".into(),
        EqEq => "==".into(),
        NotEq => "!=".into(),
        Lt => "<".into(),
        Le => "<=".into(),
        Gt => ">".into(),
        Ge => ">=".into(),
        Eq => "=".into(),
        LParen => "(".into(),
        RParen => ")".into(),
        LBrace => "{".into(),
        RBrace => "}".into(),
        Comma => ",".into(),
        Semicolon => ";".into(),
        Colon => ":".into(),
        Arrow => "->".into(),
        Eof => String::new(),
    }
}
