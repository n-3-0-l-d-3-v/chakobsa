//! A generator for arbitrary well-typed `i64`-only programs — shared by
//! the differential property test and (indirectly, via hand-picked
//! sizes) the runtime benchmark. Every generated program is a single
//! function over a fixed small set of variable names, so every
//! reference is guaranteed to resolve, and well-typed by construction
//! (every expression this grammar can produce is `i64`).
//!
//! **Bounded `while` loops, guaranteed to terminate by construction**:
//! rather than generating an arbitrary condition and hoping it becomes
//! false, every loop this generator emits has the shape
//! `let __loopN = 0; while __loopN < K { <body>; __loopN = __loopN + 1; }`
//! for a small constant `K` — the counter and its increment are always
//! generated together, never left to arbitrary body statements, so a
//! generated program can never hang a differential-test run regardless
//! of what statements land inside the loop body.

use proptest::prelude::*;

const VAR_NAMES: [&str; 3] = ["a", "b", "c"];

fn arb_expr() -> impl Strategy<Value = String> {
    let arb_atom = prop_oneof![
        (0i64..100).prop_map(|n| n.to_string()),
        prop::sample::select(&VAR_NAMES[..]).prop_map(|s| s.to_string()),
    ];
    arb_atom.prop_recursive(3, 20, 4, |inner| {
        prop_oneof![
            (inner.clone(), "[+\\-*]", inner).prop_map(|(a, op, b)| format!("({a} {op} {b})")),
        ]
    })
}

fn arb_cond() -> impl Strategy<Value = String> {
    (arb_expr(), "[<>]|==|!=", arb_expr()).prop_map(|(a, op, b)| format!("{a} {op} {b}"))
}

/// One statement, generated at a given recursion `depth` — `while` and
/// `if` are only offered while `depth > 0`, so nesting is always
/// bounded.
fn arb_stmt(depth: u32) -> impl Strategy<Value = String> {
    let assign =
        (prop::sample::select(&VAR_NAMES[..]), arb_expr()).prop_map(|(v, e)| format!("{v} = {e};"));

    if depth == 0 {
        return assign.boxed();
    }

    let if_stmt = (arb_cond(), arb_block(depth - 1))
        .prop_map(|(cond, body)| format!("if {cond} {{ {body} }}"));

    let loop_counter = format!("__loop{depth}");
    let while_stmt = (0i64..4, arb_block(depth - 1)).prop_map(move |(bound, body)| {
        format!(
            "let {c} = 0; while {c} < {bound} {{ {body} {c} = {c} + 1; }}",
            c = loop_counter
        )
    });

    prop_oneof![assign, if_stmt, while_stmt].boxed()
}

fn arb_block(depth: u32) -> impl Strategy<Value = String> {
    prop::collection::vec(arb_stmt(depth), 1..4).prop_map(|stmts| stmts.join(" "))
}

/// Generates `fn f() -> i64 { let a = 0; let b = 0; let c = 0; <body>
/// return a; }` for a bounded-depth, guaranteed-terminating `<body>`.
pub fn arb_program() -> impl Strategy<Value = String> {
    arb_block(2).prop_map(|body| {
        format!("fn f() -> i64 {{ let a = 0; let b = 0; let c = 0; {body} return a; }}")
    })
}
