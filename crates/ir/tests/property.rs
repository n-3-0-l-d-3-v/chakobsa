//! Property tests for the typed SSA IR: arbitrary straight-line
//! arithmetic programs must validate and interpret to exactly the
//! result a plain `i64` reference calculation predicts, and randomly
//! corrupting an otherwise-valid function's operand references must
//! always be caught by `validate_function` as a typed `IrError`, never
//! panic the validator itself.

use ir::{
    run, validate_function, BasicBlock, BinOp, Function, InstKind, Instruction, Module, RtValue,
    Terminator, Type, ValueId,
};
use proptest::prelude::*;

/// A tiny arithmetic expression tree, used only to generate straight-line
/// IR and an independently-computed expected `i64` result — this is the
/// test's own reference, deliberately simpler than (and built without
/// reference to) `ir::interp`.
#[derive(Debug, Clone)]
enum Expr {
    Lit(i64),
    Add(Box<Expr>, Box<Expr>),
    Sub(Box<Expr>, Box<Expr>),
    Mul(Box<Expr>, Box<Expr>),
}

fn arb_expr() -> impl Strategy<Value = Expr> {
    let leaf = (-1000i64..1000).prop_map(Expr::Lit);
    leaf.prop_recursive(4, 64, 4, |inner| {
        prop_oneof![
            (inner.clone(), inner.clone()).prop_map(|(a, b)| Expr::Add(Box::new(a), Box::new(b))),
            (inner.clone(), inner.clone()).prop_map(|(a, b)| Expr::Sub(Box::new(a), Box::new(b))),
            (inner.clone(), inner).prop_map(|(a, b)| Expr::Mul(Box::new(a), Box::new(b))),
        ]
    })
}

fn expected(e: &Expr) -> i64 {
    match e {
        Expr::Lit(v) => *v,
        Expr::Add(a, b) => expected(a).wrapping_add(expected(b)),
        Expr::Sub(a, b) => expected(a).wrapping_sub(expected(b)),
        Expr::Mul(a, b) => expected(a).wrapping_mul(expected(b)),
    }
}

/// Lowers `e` into `block`'s instruction list, returning the value id
/// holding the result. `next_id` is threaded through by `&mut` so every
/// emitted value gets a fresh, never-reused id, as SSA requires.
fn lower(e: &Expr, block: &mut BasicBlock, next_id: &mut u32) -> ValueId {
    let id = ValueId(*next_id);
    *next_id += 1;
    let kind = match e {
        Expr::Lit(v) => InstKind::ConstI64(*v),
        Expr::Add(a, b) => {
            let lhs = lower(a, block, next_id);
            let rhs = lower(b, block, next_id);
            InstKind::Bin(BinOp::Add, lhs, rhs)
        }
        Expr::Sub(a, b) => {
            let lhs = lower(a, block, next_id);
            let rhs = lower(b, block, next_id);
            InstKind::Bin(BinOp::Sub, lhs, rhs)
        }
        Expr::Mul(a, b) => {
            let lhs = lower(a, block, next_id);
            let rhs = lower(b, block, next_id);
            InstKind::Bin(BinOp::Mul, lhs, rhs)
        }
    };
    // Reserve `id` before recursing so a node's id is always smaller
    // than its children's, then push its own instruction after (post-
    // order) so it lands in the instruction list strictly after both
    // operands' defining instructions — textual def-before-use order is
    // what validation actually needs, not numeric id order.
    block.instructions.push(Instruction {
        id,
        ty: Type::I64,
        kind,
    });
    id
}

fn build_module(e: &Expr) -> (Module, ValueId) {
    let mut block = BasicBlock::new(ir::BlockId(0));
    let mut next_id = 0u32;
    let result = lower(e, &mut block, &mut next_id);
    block.terminator = Some(Terminator::Return(Some(result)));

    let func = Function {
        name: "f".to_string(),
        params: vec![],
        ret_ty: Some(Type::I64),
        blocks: vec![block],
        entry: ir::BlockId(0),
    };
    (
        Module {
            functions: vec![func],
        },
        result,
    )
}

proptest! {
    /// Arbitrary straight-line arithmetic expressions, lowered directly
    /// to IR (no parser involved yet — ticket 003 will replace this
    /// hand-lowering), must validate and interpret to exactly the
    /// independently-computed expected result.
    #[test]
    fn arbitrary_arithmetic_validates_and_interprets_correctly(expr in arb_expr()) {
        let (module, _) = build_module(&expr);
        prop_assert_eq!(validate_function(&module.functions[0]), Ok(()));
        let result = run(&module, "f", &[]).unwrap();
        prop_assert_eq!(result, Some(RtValue::I64(expected(&expr))));
    }

    /// Corrupting a valid, non-trivial function's operand references to
    /// point at an id that was never defined must always be caught by
    /// `validate_function` as `IrError::UndefinedValue`, never panic the
    /// validator and never be silently accepted.
    #[test]
    fn an_undefined_operand_reference_is_always_caught(
        expr in arb_expr(),
        bogus_offset in 1000u32..2000,
    ) {
        let (module, _) = build_module(&expr);
        let mut func = module.functions[0].clone();
        // Corrupt the first Bin instruction's left operand, if any exists,
        // to reference a value id that's guaranteed never to have been
        // defined (well past every id this small a program could emit).
        let corrupted = func.blocks[0].instructions.iter_mut().find_map(|inst| {
            if let InstKind::Bin(_, lhs, _) = &mut inst.kind {
                *lhs = ValueId(bogus_offset);
                Some(())
            } else {
                None
            }
        });
        prop_assume!(corrupted.is_some());
        let result = validate_function(&func);
        let is_undefined_value_error = matches!(result, Err(ir::IrError::UndefinedValue { .. }));
        prop_assert!(is_undefined_value_error, "expected UndefinedValue, got {:?}", result);
    }
}
