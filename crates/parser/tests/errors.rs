//! Error-path tests: every kind of malformed or ill-typed source this
//! parser is supposed to reject must produce a typed `ParseError`, never
//! a panic — type checking folded into construction means these are
//! parse-time errors, not a later pass (`docs/design/LANGUAGE.md`).

use parser::{parse, ParseError};

#[test]
fn using_an_undefined_variable_is_a_typed_error() {
    let result = parse("fn f() -> i64 { return y; }");
    assert!(matches!(result, Err(ParseError::UndefinedVariable { name, .. }) if name == "y"));
}

#[test]
fn calling_an_undefined_function_is_a_typed_error() {
    let result = parse("fn f() -> i64 { return g(1); }");
    assert!(matches!(result, Err(ParseError::UndefinedFunction { name, .. }) if name == "g"));
}

#[test]
fn wrong_argument_count_is_a_typed_error() {
    let result = parse(
        r#"
        fn g(a: i64, b: i64) -> i64 { return a + b; }
        fn f() -> i64 { return g(1); }
        "#,
    );
    assert!(matches!(
        result,
        Err(ParseError::ArgCountMismatch { func, expected: 2, given: 1, .. }) if func == "g"
    ));
}

#[test]
fn wrong_argument_type_is_a_typed_error() {
    let result = parse(
        r#"
        fn g(a: i64) -> i64 { return a; }
        fn f() -> i64 { return g(true); }
        "#,
    );
    assert!(matches!(result, Err(ParseError::TypeMismatch { .. })));
}

#[test]
fn adding_a_bool_to_an_i64_is_a_typed_error() {
    let result = parse("fn f() -> i64 { return 1 + true; }");
    assert!(matches!(
        result,
        Err(ParseError::TypeMismatch {
            expected: ir::Type::I64,
            actual: ir::Type::Bool,
            ..
        })
    ));
}

#[test]
fn a_non_bool_if_condition_is_a_typed_error() {
    let result = parse("fn f() -> i64 { if 1 { return 1; } return 0; }");
    assert!(matches!(
        result,
        Err(ParseError::TypeMismatch {
            expected: ir::Type::Bool,
            actual: ir::Type::I64,
            ..
        })
    ));
}

#[test]
fn a_non_bool_while_condition_is_a_typed_error() {
    let result = parse("fn f() -> i64 { while 1 { return 0; } return 0; }");
    assert!(matches!(
        result,
        Err(ParseError::TypeMismatch {
            expected: ir::Type::Bool,
            actual: ir::Type::I64,
            ..
        })
    ));
}

#[test]
fn returning_the_wrong_type_is_a_typed_error() {
    let result = parse("fn f() -> i64 { return true; }");
    assert!(matches!(
        result,
        Err(ParseError::TypeMismatch {
            expected: ir::Type::I64,
            actual: ir::Type::Bool,
            ..
        })
    ));
}

#[test]
fn assigning_to_an_undefined_variable_is_a_typed_error() {
    let result = parse("fn f() -> i64 { y = 1; return 0; }");
    assert!(matches!(result, Err(ParseError::UndefinedVariable { name, .. }) if name == "y"));
}

#[test]
fn assigning_the_wrong_type_to_an_existing_variable_is_a_typed_error() {
    let result = parse("fn f() -> i64 { let x = 1; x = true; return x; }");
    assert!(matches!(result, Err(ParseError::TypeMismatch { .. })));
}

#[test]
fn a_duplicate_function_name_is_a_typed_error() {
    let result = parse("fn f() -> i64 { return 1; } fn f() -> i64 { return 2; }");
    assert!(matches!(result, Err(ParseError::DuplicateFunction { name }) if name == "f"));
}

#[test]
fn a_duplicate_parameter_name_is_a_typed_error() {
    let result = parse("fn f(a: i64, a: i64) -> i64 { return a; }");
    assert!(matches!(result, Err(ParseError::DuplicateParameter { name, .. }) if name == "a"));
}

#[test]
fn code_after_a_return_in_the_same_block_is_rejected_as_unreachable() {
    let result = parse("fn f() -> i64 { return 1; let x = 2; return x; }");
    assert!(matches!(result, Err(ParseError::UnreachableCode { .. })));
}

#[test]
fn an_unexpected_token_is_a_typed_error_not_a_panic() {
    let result = parse("fn f() -> i64 { return ; }");
    assert!(matches!(result, Err(ParseError::UnexpectedToken { .. })));
}

#[test]
fn a_lexical_error_propagates_as_a_typed_parse_error() {
    let result = parse("fn f() -> i64 { return @; }");
    assert!(matches!(result, Err(ParseError::Lex(_))));
}

#[test]
fn an_unclosed_block_is_a_typed_error_not_a_panic() {
    let result = parse("fn f() -> i64 { return 1;");
    assert!(result.is_err());
}

#[test]
fn a_program_with_both_branches_of_an_if_returning_and_nothing_after_it_is_accepted() {
    // Both arms terminate; there's nothing else in the function, which
    // is fine (see the parser's handling of the both-terminated case).
    let result = parse(
        r#"
        fn f(x: i64) -> i64 {
            if x < 0 {
                return -1;
            } else {
                return 1;
            }
        }
        "#,
    );
    assert!(result.is_ok(), "{result:?}");
}
