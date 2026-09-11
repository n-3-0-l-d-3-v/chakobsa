//! Property tests for the parser: arbitrary generated well-typed
//! programs must always produce IR that passes `ir::validate_module` —
//! the parser is supposed to make ill-formed IR structurally impossible
//! to emit, not just usually avoid it — and completely arbitrary text
//! must never panic the parser, only ever return `Ok` or a typed
//! `ParseError`.

use ir::validate_module;
use parser::parse;
use proptest::prelude::*;

proptest! {
    /// Arbitrary UTF-8-lossy byte input must never panic the lexer or
    /// parser — only ever `Ok` or a typed `ParseError`.
    #[test]
    fn parse_never_panics_on_arbitrary_input(bytes in prop::collection::vec(any::<u8>(), 0..300)) {
        let source = String::from_utf8_lossy(&bytes);
        let _ = parse(&source);
    }

    /// A small generated grammar of well-typed straight-line-plus-branch
    /// programs must always produce IR that validates — the parser's
    /// central job is to make this structurally guaranteed, not just
    /// usually true.
    #[test]
    fn arbitrary_generated_programs_always_produce_valid_ir(src in arb_program()) {
        let module = parse(&src).unwrap_or_else(|e| panic!("generated program failed to parse: {e}\n{src}"));
        prop_assert_eq!(validate_module(&module), Ok(()), "generated program's IR failed validation:\n{}", src);
    }
}

/// Generates small, well-typed `i64`-only programs: a single function
/// with a handful of `let`/`if`/`while`/arithmetic statements over a
/// fixed small set of variable names, so every reference is guaranteed
/// to resolve. Bounded depth/length keeps generation and validation
/// fast, matching this project's existing bounded-property-test
/// convention for anything doing real I/O or nontrivial construction
/// work per case (see e.g. sietch's `compaction_property.rs`).
fn arb_program() -> impl Strategy<Value = String> {
    static VAR_NAMES: [&str; 3] = ["a", "b", "c"];

    let arb_atom = prop_oneof![
        (0i64..100).prop_map(|n| n.to_string()),
        prop::sample::select(&VAR_NAMES[..]).prop_map(|s| s.to_string()),
    ];

    let arb_expr = arb_atom.prop_recursive(3, 20, 4, |inner| {
        prop_oneof![(inner.clone(), "[+\\-*]", inner.clone())
            .prop_map(|(a, op, b)| format!("({a} {op} {b})")),]
    });

    let arb_cond = (arb_expr.clone(), "[<>]|==|!=", arb_expr.clone())
        .prop_map(|(a, op, b)| format!("{a} {op} {b}"));

    let arb_stmt = prop_oneof![
        (prop::sample::select(&VAR_NAMES[..]), arb_expr.clone())
            .prop_map(|(v, e)| format!("{v} = {e};")),
        arb_cond.clone().prop_flat_map(move |cond| {
            let v = VAR_NAMES[0];
            (Just(cond), 0i64..50).prop_map(move |(cond, n)| format!("if {cond} {{ {v} = {n}; }}"))
        }),
    ];

    prop::collection::vec(arb_stmt, 1..8).prop_map(move |stmts| {
        format!(
            "fn f() -> i64 {{ let a = 0; let b = 0; let c = 0; {} return a; }}",
            stmts.join(" ")
        )
    })
}
