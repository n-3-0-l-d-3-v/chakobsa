//! End-to-end tests: real `.ck`-shaped source text, parsed straight to
//! typed SSA (no AST stage — see `docs/design/LANGUAGE.md`), validated,
//! and run through `ir::run` to a hand-verified expected result. This is
//! the ticket 003 acceptance bar: representative programs exercising
//! straight-line arithmetic, branches, loops, short-circuit and/or, and
//! (mutual) recursion.

use ir::{run, validate_module, RtValue};
use parser::parse;

fn exec(src: &str, entry: &str, args: &[RtValue]) -> RtValue {
    let module = parse(src).unwrap_or_else(|e| panic!("parse error: {e}"));
    validate_module(&module).unwrap_or_else(|e| panic!("ir failed validation: {e}"));
    run(&module, entry, args)
        .unwrap_or_else(|e| panic!("interpreter error: {e}"))
        .expect("function has a return type, so it must return a value")
}

#[test]
fn straight_line_arithmetic() {
    let src = "fn add(a: i64, b: i64) -> i64 { return a + b; }";
    assert_eq!(
        exec(src, "add", &[RtValue::I64(3), RtValue::I64(4)]),
        RtValue::I64(7)
    );
}

#[test]
fn operator_precedence_matches_conventional_arithmetic() {
    // 2 + 3 * 4 - 1 == 13, not (2+3)*(4-1) == 15.
    let src = "fn f() -> i64 { return 2 + 3 * 4 - 1; }";
    assert_eq!(exec(src, "f", &[]), RtValue::I64(13));
}

#[test]
fn if_else_branch_and_phi() {
    let src = r#"
        fn abs(x: i64) -> i64 {
            if x < 0 {
                return -x;
            } else {
                return x;
            }
        }
    "#;
    assert_eq!(exec(src, "abs", &[RtValue::I64(-7)]), RtValue::I64(7));
    assert_eq!(exec(src, "abs", &[RtValue::I64(7)]), RtValue::I64(7));
}

#[test]
fn if_without_else_falls_through_to_a_real_phi() {
    let src = r#"
        fn clamp_positive(x: i64) -> i64 {
            let y = x;
            if x < 0 {
                y = 0;
            }
            return y;
        }
    "#;
    assert_eq!(
        exec(src, "clamp_positive", &[RtValue::I64(-5)]),
        RtValue::I64(0)
    );
    assert_eq!(
        exec(src, "clamp_positive", &[RtValue::I64(5)]),
        RtValue::I64(5)
    );
}

#[test]
fn while_loop_with_a_loop_carried_variable() {
    let src = r#"
        fn sum_to(n: i64) -> i64 {
            let acc = 0;
            let i = 0;
            while i < n {
                acc = acc + i;
                i = i + 1;
            }
            return acc;
        }
    "#;
    assert_eq!(exec(src, "sum_to", &[RtValue::I64(5)]), RtValue::I64(10)); // 0+1+2+3+4
    assert_eq!(exec(src, "sum_to", &[RtValue::I64(0)]), RtValue::I64(0));
    assert_eq!(exec(src, "sum_to", &[RtValue::I64(1)]), RtValue::I64(0));
}

#[test]
fn nested_while_loops() {
    let src = r#"
        fn count_pairs(n: i64) -> i64 {
            let count = 0;
            let i = 0;
            while i < n {
                let j = 0;
                while j < n {
                    count = count + 1;
                    j = j + 1;
                }
                i = i + 1;
            }
            return count;
        }
    "#;
    assert_eq!(
        exec(src, "count_pairs", &[RtValue::I64(3)]),
        RtValue::I64(9)
    );
}

#[test]
fn recursive_function() {
    let src = r#"
        fn fact(n: i64) -> i64 {
            if n <= 1 {
                return 1;
            } else {
                return n * fact(n - 1);
            }
        }
    "#;
    assert_eq!(exec(src, "fact", &[RtValue::I64(5)]), RtValue::I64(120));
}

#[test]
fn mutually_recursive_functions_resolve_via_the_forward_signature_pass() {
    // is_even calls is_odd, which is declared *after* it in the source —
    // only resolvable because signatures are scanned before any body is
    // parsed (see Parser::parse_module's two passes).
    let src = r#"
        fn is_even(n: i64) -> bool {
            if n == 0 {
                return true;
            } else {
                return is_odd(n - 1);
            }
        }
        fn is_odd(n: i64) -> bool {
            if n == 0 {
                return false;
            } else {
                return is_even(n - 1);
            }
        }
    "#;
    let module = parse(src).unwrap();
    validate_module(&module).unwrap();
    assert_eq!(
        run(&module, "is_even", &[RtValue::I64(10)]).unwrap(),
        Some(RtValue::Bool(true))
    );
    assert_eq!(
        run(&module, "is_odd", &[RtValue::I64(10)]).unwrap(),
        Some(RtValue::Bool(false))
    );
}

#[test]
fn short_circuit_and_never_evaluates_the_rhs_when_the_lhs_is_false() {
    // If short-circuiting weren't real, dividing by the zero-when-false
    // path would trip a runtime DivideByZero instead of returning false.
    let src = r#"
        fn f(x: i64) -> bool {
            return x != 0 and 10 / x > 1;
        }
    "#;
    assert_eq!(exec(src, "f", &[RtValue::I64(0)]), RtValue::Bool(false)); // short-circuits, never divides
    assert_eq!(exec(src, "f", &[RtValue::I64(1)]), RtValue::Bool(true)); // 10/1=10, 10>1
    assert_eq!(exec(src, "f", &[RtValue::I64(20)]), RtValue::Bool(false)); // 10/20=0, 0>1 is false
}

#[test]
fn short_circuit_or_never_evaluates_the_rhs_when_the_lhs_is_true() {
    let src = r#"
        fn f(x: i64) -> bool {
            return x == 0 or 10 / x > 1;
        }
    "#;
    assert_eq!(exec(src, "f", &[RtValue::I64(0)]), RtValue::Bool(true));
    assert_eq!(exec(src, "f", &[RtValue::I64(20)]), RtValue::Bool(false));
    assert_eq!(exec(src, "f", &[RtValue::I64(1)]), RtValue::Bool(true));
}

#[test]
fn not_and_boolean_literals() {
    let src = "fn f(x: bool) -> bool { return not x; }";
    assert_eq!(exec(src, "f", &[RtValue::Bool(true)]), RtValue::Bool(false));
    assert_eq!(exec(src, "f", &[RtValue::Bool(false)]), RtValue::Bool(true));
}

#[test]
fn a_call_used_inside_a_larger_expression() {
    let src = r#"
        fn square(x: i64) -> i64 { return x * x; }
        fn sum_of_squares(a: i64, b: i64) -> i64 { return square(a) + square(b); }
    "#;
    assert_eq!(
        exec(src, "sum_of_squares", &[RtValue::I64(3), RtValue::I64(4)]),
        RtValue::I64(25)
    );
}

#[test]
fn comments_and_whitespace_are_ignored() {
    let src =
        "// header comment\nfn f() -> i64 {\n  // body comment\n  return 42; // trailing\n}\n";
    assert_eq!(exec(src, "f", &[]), RtValue::I64(42));
}
