//! A reference interpreter that walks the typed SSA CFG directly —
//! deliberately independent of codegen (ticket 004) and mentat, so
//! ticket 006's differential tests have a genuinely separate
//! implementation to compare compiled-and-VM-executed results against,
//! not the same code path measuring itself.
//!
//! Not a *tree*-walking interpreter (there is no tree in this pipeline —
//! see `docs/design/LANGUAGE.md`); it's a CFG-walking interpreter:
//! execute a block's instructions in order, then follow its terminator
//! to the next block (remembering which block it came from, needed to
//! resolve a `Phi` in the block it jumps to), until a `Return`.

use std::collections::HashMap;

use crate::block::Terminator;
use crate::inst::{BinOp, InstKind, UnOp};
use crate::module::Module;
use crate::types::Type;
use crate::value::{BlockId, ValueId};
use crate::Function;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RtValue {
    I64(i64),
    Bool(bool),
}

impl RtValue {
    pub fn ty(self) -> Type {
        match self {
            RtValue::I64(_) => Type::I64,
            RtValue::Bool(_) => Type::Bool,
        }
    }

    fn as_i64(self) -> i64 {
        match self {
            RtValue::I64(v) => v,
            RtValue::Bool(_) => panic!(
                "interpreter bug: expected I64, got Bool (should have been caught by ir::validate)"
            ),
        }
    }

    fn as_bool(self) -> bool {
        match self {
            RtValue::Bool(v) => v,
            RtValue::I64(_) => panic!(
                "interpreter bug: expected Bool, got I64 (should have been caught by ir::validate)"
            ),
        }
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum InterpError {
    #[error("call to unknown function '{0}'")]
    UnknownFunction(String),
    #[error("division by zero")]
    DivideByZero,
    #[error("wrong number of arguments calling '{func}': expected {expected}, got {given}")]
    ArgCountMismatch {
        func: String,
        expected: usize,
        given: usize,
    },
}

/// Interprets `module`'s function named `entry` with `args`, matched
/// positionally against its parameters. Assumes `ir::validate_module`
/// already accepted `module` — this does not re-check types or
/// structure, only the runtime-only failure mode `validate` can't catch
/// statically (division by zero).
pub fn run(module: &Module, entry: &str, args: &[RtValue]) -> Result<Option<RtValue>, InterpError> {
    let func = module
        .function(entry)
        .ok_or_else(|| InterpError::UnknownFunction(entry.to_string()))?;
    call(module, func, args)
}

fn call(
    module: &Module,
    func: &Function,
    args: &[RtValue],
) -> Result<Option<RtValue>, InterpError> {
    if args.len() != func.params.len() {
        return Err(InterpError::ArgCountMismatch {
            func: func.name.clone(),
            expected: func.params.len(),
            given: args.len(),
        });
    }

    let mut env: HashMap<ValueId, RtValue> = HashMap::new();
    // Parameters occupy the first `params.len()` value ids by convention
    // (ticket 003's parser assigns them this way when it builds a
    // function's entry block); the interpreter and codegen (ticket 004)
    // both rely on this rather than re-deriving it independently.
    for (i, arg) in args.iter().enumerate() {
        env.insert(ValueId(i as u32), *arg);
    }

    let mut current = func.entry;
    let mut came_from: Option<BlockId> = None;

    loop {
        let block = func
            .block(current)
            .expect("validated module: block id always resolves");

        for inst in &block.instructions {
            let value = match &inst.kind {
                InstKind::ConstI64(v) => RtValue::I64(*v),
                InstKind::ConstBool(v) => RtValue::Bool(*v),
                InstKind::Bin(op, lhs, rhs) => eval_bin(*op, env[lhs], env[rhs])?,
                InstKind::Un(op, operand) => eval_un(*op, env[operand]),
                InstKind::Phi(incoming) => {
                    let from = came_from.expect("a Phi never appears in the entry block");
                    let (_, v) = incoming
                        .iter()
                        .find(|(b, _)| *b == from)
                        .expect("validated module: phi covers every predecessor");
                    env[v]
                }
                InstKind::Call { func: callee, args } => {
                    let callee_fn = module
                        .function(callee)
                        .ok_or_else(|| InterpError::UnknownFunction(callee.clone()))?;
                    let call_args: Vec<RtValue> = args.iter().map(|a| env[a]).collect();
                    call(module, callee_fn, &call_args)?.expect(
                        "validated module: a Call used as a value targets a non-void function",
                    )
                }
            };
            env.insert(inst.id, value);
        }

        match block
            .terminator
            .as_ref()
            .expect("validated module: every block has a terminator")
        {
            Terminator::Jump(target) => {
                came_from = Some(current);
                current = *target;
            }
            Terminator::Branch {
                cond,
                then_block,
                else_block,
            } => {
                came_from = Some(current);
                current = if env[cond].as_bool() {
                    *then_block
                } else {
                    *else_block
                };
            }
            Terminator::Return(value) => {
                return Ok(value.map(|v| env[&v]));
            }
        }
    }
}

fn eval_bin(op: BinOp, lhs: RtValue, rhs: RtValue) -> Result<RtValue, InterpError> {
    use BinOp::*;
    Ok(match op {
        Add => RtValue::I64(lhs.as_i64().wrapping_add(rhs.as_i64())),
        Sub => RtValue::I64(lhs.as_i64().wrapping_sub(rhs.as_i64())),
        Mul => RtValue::I64(lhs.as_i64().wrapping_mul(rhs.as_i64())),
        Div => {
            let (a, b) = (lhs.as_i64(), rhs.as_i64());
            if b == 0 {
                return Err(InterpError::DivideByZero);
            }
            RtValue::I64(a.wrapping_div(b))
        }
        Mod => {
            let (a, b) = (lhs.as_i64(), rhs.as_i64());
            if b == 0 {
                return Err(InterpError::DivideByZero);
            }
            RtValue::I64(a.wrapping_rem(b))
        }
        CmpEq => RtValue::Bool(lhs.as_i64() == rhs.as_i64()),
        CmpNe => RtValue::Bool(lhs.as_i64() != rhs.as_i64()),
        CmpLt => RtValue::Bool(lhs.as_i64() < rhs.as_i64()),
        CmpLe => RtValue::Bool(lhs.as_i64() <= rhs.as_i64()),
        CmpGt => RtValue::Bool(lhs.as_i64() > rhs.as_i64()),
        CmpGe => RtValue::Bool(lhs.as_i64() >= rhs.as_i64()),
        And => RtValue::Bool(lhs.as_bool() && rhs.as_bool()),
        Or => RtValue::Bool(lhs.as_bool() || rhs.as_bool()),
    })
}

fn eval_un(op: UnOp, v: RtValue) -> RtValue {
    match op {
        UnOp::Neg => RtValue::I64(v.as_i64().wrapping_neg()),
        UnOp::Not => RtValue::Bool(!v.as_bool()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block::Terminator;
    use crate::inst::{BinOp, InstKind, UnOp};
    use crate::test_support::FnBuilder;

    /// fn add(a: i64, b: i64) -> i64 { return a + b; }
    fn build_add() -> Module {
        let mut b = FnBuilder::new(
            "add",
            vec![("a", Type::I64), ("b", Type::I64)],
            Some(Type::I64),
        );
        let entry = b.new_block();
        b.set_entry(entry);
        let sum = b.push(
            entry,
            Type::I64,
            InstKind::Bin(BinOp::Add, b.param_value(0), b.param_value(1)),
        );
        b.terminate(entry, Terminator::Return(Some(sum)));
        Module {
            functions: vec![b.finish()],
        }
    }

    #[test]
    fn straight_line_arithmetic_runs() {
        let module = build_add();
        let result = run(&module, "add", &[RtValue::I64(3), RtValue::I64(4)]).unwrap();
        assert_eq!(result, Some(RtValue::I64(7)));
    }

    #[test]
    fn a_branch_with_a_phi_resolves_the_correct_incoming_value() {
        // fn abs(x: i64) -> i64 {
        //   if x < 0 { y = -x } else { y = x }
        //   return y;
        // }
        let mut b = FnBuilder::new("abs", vec![("x", Type::I64)], Some(Type::I64));
        let entry = b.new_block();
        let neg_branch = b.new_block();
        let pos_branch = b.new_block();
        let join = b.new_block();
        b.set_entry(entry);

        let zero = b.push(entry, Type::I64, InstKind::ConstI64(0));
        let is_neg = b.push(
            entry,
            Type::Bool,
            InstKind::Bin(BinOp::CmpLt, b.param_value(0), zero),
        );
        b.terminate(
            entry,
            Terminator::Branch {
                cond: is_neg,
                then_block: neg_branch,
                else_block: pos_branch,
            },
        );
        let negated = b.push(
            neg_branch,
            Type::I64,
            InstKind::Un(UnOp::Neg, b.param_value(0)),
        );
        b.terminate(neg_branch, Terminator::Jump(join));
        b.terminate(pos_branch, Terminator::Jump(join));
        let phi = b.push(
            join,
            Type::I64,
            InstKind::Phi(vec![(neg_branch, negated), (pos_branch, b.param_value(0))]),
        );
        b.terminate(join, Terminator::Return(Some(phi)));

        let module = Module {
            functions: vec![b.finish()],
        };
        assert_eq!(
            run(&module, "abs", &[RtValue::I64(-5)]).unwrap(),
            Some(RtValue::I64(5))
        );
        assert_eq!(
            run(&module, "abs", &[RtValue::I64(5)]).unwrap(),
            Some(RtValue::I64(5))
        );
    }

    #[test]
    fn a_while_loop_accumulates_correctly() {
        // fn sum_to(n: i64) -> i64 {
        //   let acc = 0; let i = 0;
        //   while i < n { acc = acc + i; i = i + 1; }
        //   return acc;
        // }
        let mut b = FnBuilder::new("sum_to", vec![("n", Type::I64)], Some(Type::I64));
        let entry = b.new_block();
        let header = b.new_block();
        let body = b.new_block();
        let exit = b.new_block();
        b.set_entry(entry);

        let zero = b.push(entry, Type::I64, InstKind::ConstI64(0));
        b.terminate(entry, Terminator::Jump(header));

        // header: acc = phi(0 from entry, acc_next from body)
        //         i   = phi(0 from entry, i_next from body)
        let acc_phi_id = ValueId(10);
        let i_phi_id = ValueId(11);
        b.push_with_id(header, acc_phi_id, Type::I64, InstKind::Phi(vec![]));
        b.push_with_id(header, i_phi_id, Type::I64, InstKind::Phi(vec![]));
        let cond = b.push(
            header,
            Type::Bool,
            InstKind::Bin(BinOp::CmpLt, i_phi_id, b.param_value(0)),
        );
        b.terminate(
            header,
            Terminator::Branch {
                cond,
                then_block: body,
                else_block: exit,
            },
        );

        let acc_next = b.push(
            body,
            Type::I64,
            InstKind::Bin(BinOp::Add, acc_phi_id, i_phi_id),
        );
        let one = b.push(body, Type::I64, InstKind::ConstI64(1));
        let i_next = b.push(body, Type::I64, InstKind::Bin(BinOp::Add, i_phi_id, one));
        b.terminate(body, Terminator::Jump(header));

        b.terminate(exit, Terminator::Return(Some(acc_phi_id)));

        let mut func = b.finish();
        // Patch the phis' incoming lists now that entry/body are known
        // (FnBuilder has no phi-patching helper; this mirrors what
        // ticket 003's real SSA-construction builder will do inline).
        for block in &mut func.blocks {
            for inst in &mut block.instructions {
                if inst.id == acc_phi_id {
                    inst.kind = InstKind::Phi(vec![(entry, zero), (body, acc_next)]);
                }
                if inst.id == i_phi_id {
                    inst.kind = InstKind::Phi(vec![(entry, zero), (body, i_next)]);
                }
            }
        }

        crate::validate::validate_function(&func).expect("hand-built loop must validate");

        let module = Module {
            functions: vec![func],
        };
        // sum_to(5) = 0+1+2+3+4 = 10
        assert_eq!(
            run(&module, "sum_to", &[RtValue::I64(5)]).unwrap(),
            Some(RtValue::I64(10))
        );
        assert_eq!(
            run(&module, "sum_to", &[RtValue::I64(0)]).unwrap(),
            Some(RtValue::I64(0))
        );
    }

    #[test]
    fn a_call_to_another_function_runs_it() {
        let mut callee = FnBuilder::new("double", vec![("x", Type::I64)], Some(Type::I64));
        let ce = callee.new_block();
        callee.set_entry(ce);
        let two = callee.push(ce, Type::I64, InstKind::ConstI64(2));
        let doubled = callee.push(
            ce,
            Type::I64,
            InstKind::Bin(BinOp::Mul, callee.param_value(0), two),
        );
        callee.terminate(ce, Terminator::Return(Some(doubled)));

        let mut caller = FnBuilder::new("caller", vec![], Some(Type::I64));
        let cae = caller.new_block();
        caller.set_entry(cae);
        let twenty_one = caller.push(cae, Type::I64, InstKind::ConstI64(21));
        let result = caller.push(
            cae,
            Type::I64,
            InstKind::Call {
                func: "double".into(),
                args: vec![twenty_one],
            },
        );
        caller.terminate(cae, Terminator::Return(Some(result)));

        let module = Module {
            functions: vec![callee.finish(), caller.finish()],
        };
        assert_eq!(run(&module, "caller", &[]).unwrap(), Some(RtValue::I64(42)));
    }

    #[test]
    fn division_by_zero_is_a_typed_runtime_error_not_a_panic() {
        let mut b = FnBuilder::new("f", vec![], Some(Type::I64));
        let entry = b.new_block();
        b.set_entry(entry);
        let ten = b.push(entry, Type::I64, InstKind::ConstI64(10));
        let zero = b.push(entry, Type::I64, InstKind::ConstI64(0));
        let result = b.push(entry, Type::I64, InstKind::Bin(BinOp::Div, ten, zero));
        b.terminate(entry, Terminator::Return(Some(result)));
        let module = Module {
            functions: vec![b.finish()],
        };
        assert_eq!(run(&module, "f", &[]), Err(InterpError::DivideByZero));
    }

    #[test]
    fn calling_an_unknown_entry_function_is_a_typed_error() {
        let module = Module::default();
        assert_eq!(
            run(&module, "nonexistent", &[]),
            Err(InterpError::UnknownFunction("nonexistent".into()))
        );
    }

    #[test]
    fn wrong_argument_count_is_a_typed_error() {
        let module = build_add();
        assert_eq!(
            run(&module, "add", &[RtValue::I64(1)]),
            Err(InterpError::ArgCountMismatch {
                func: "add".into(),
                expected: 2,
                given: 1,
            })
        );
    }
}
