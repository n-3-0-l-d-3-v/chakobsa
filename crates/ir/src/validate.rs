//! Structural + type validation for the typed SSA IR — analogous to
//! mentat's `Program::validate` (`docs/design/LANGUAGE.md`): every
//! structural defect is caught here, once, before codegen or the
//! reference interpreter ever sees the IR, rather than surfacing as a
//! confusing downstream panic or miscompile.
//!
//! **Known limitation, documented rather than silently assumed away**:
//! an operand's defining instruction is only checked to exist
//! *somewhere* in the function, not that it dominates the use (full SSA
//! well-formedness needs dominance). Ticket 003's parser only ever
//! constructs IR where this holds by construction (Braun et al.'s
//! algorithm resolves reads against exactly what's live at that point in
//! the CFG being built), so this is a real but currently-unexercised gap
//! — tracked here rather than assumed impossible.

use std::collections::{HashMap, HashSet};

use crate::block::Terminator;
use crate::inst::InstKind;
use crate::module::Module;
use crate::types::Type;
use crate::value::{BlockId, ValueId};
use crate::Function;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum IrError {
    #[error("function '{func}': block {block} is empty and has no terminator")]
    EmptyBlock { func: String, block: BlockId },
    #[error("function '{func}': block {block} has no terminator")]
    MissingTerminator { func: String, block: BlockId },
    #[error("function '{func}': value {value} is defined more than once")]
    DuplicateValue { func: String, value: ValueId },
    #[error("function '{func}': block {block} is defined more than once")]
    DuplicateBlock { func: String, block: BlockId },
    #[error("function '{func}': entry block {block} does not exist")]
    UnknownEntryBlock { func: String, block: BlockId },
    #[error("function '{func}': terminator in block {block} refers to unknown block {target}")]
    UnknownBlock {
        func: String,
        block: BlockId,
        target: BlockId,
    },
    #[error("function '{func}': block {block} uses undefined value {value}")]
    UndefinedValue {
        func: String,
        block: BlockId,
        value: ValueId,
    },
    #[error(
        "function '{func}': block {block}, value {value}: expected type {expected}, got {actual}"
    )]
    TypeMismatch {
        func: String,
        block: BlockId,
        value: ValueId,
        expected: Type,
        actual: Type,
    },
    #[error(
        "function '{func}': block {block}, phi {value} has {given} incoming value(s) but the block has {expected} predecessor(s)"
    )]
    PhiArityMismatch {
        func: String,
        block: BlockId,
        value: ValueId,
        expected: usize,
        given: usize,
    },
    #[error(
        "function '{func}': block {block}, phi {value} names incoming block {named} which is not one of this block's actual predecessors"
    )]
    PhiUnknownPredecessor {
        func: String,
        block: BlockId,
        value: ValueId,
        named: BlockId,
    },
    #[error(
        "function '{func}': block {block} returns a value but the function has no return type"
    )]
    UnexpectedReturnValue { func: String, block: BlockId },
    #[error(
        "function '{func}': block {block} returns nothing but the function's return type is {expected}"
    )]
    MissingReturnValue {
        func: String,
        block: BlockId,
        expected: Type,
    },
    #[error("call to unknown function '{callee}' in function '{func}', block {block}")]
    UnknownCallee {
        func: String,
        block: BlockId,
        callee: String,
    },
    #[error(
        "call to '{callee}' in function '{func}', block {block}: expected {expected} argument(s), got {given}"
    )]
    CallArityMismatch {
        func: String,
        block: BlockId,
        callee: String,
        expected: usize,
        given: usize,
    },
    #[error(
        "function '{func}', block {block}: call to '{callee}' is used as a value, but '{callee}' has no return type"
    )]
    CallToVoidFunctionUsedAsValue {
        func: String,
        block: BlockId,
        callee: String,
    },
}

/// Every value a function defines, mapped to its declared type. Also
/// the single source of truth `validate_function` uses to catch a
/// duplicate definition (two instructions claiming the same `ValueId`).
///
/// Parameters are pre-defined values by convention: they occupy
/// `ValueId(0)..ValueId(params.len())` before any instruction runs (see
/// `interp::call`'s matching convention) — so they seed this map, not
/// just instructions.
fn collect_definitions(func: &Function) -> Result<HashMap<ValueId, Type>, IrError> {
    let mut defs = HashMap::new();
    for (i, (_, ty)) in func.params.iter().enumerate() {
        defs.insert(ValueId(i as u32), *ty);
    }
    for block in &func.blocks {
        for inst in &block.instructions {
            if defs.insert(inst.id, inst.ty).is_some() {
                return Err(IrError::DuplicateValue {
                    func: func.name.clone(),
                    value: inst.id,
                });
            }
        }
    }
    Ok(defs)
}

fn check_operand(
    func: &Function,
    block: BlockId,
    defs: &HashMap<ValueId, Type>,
    operand: ValueId,
    expected: Type,
) -> Result<(), IrError> {
    let actual = *defs.get(&operand).ok_or(IrError::UndefinedValue {
        func: func.name.clone(),
        block,
        value: operand,
    })?;
    if actual != expected {
        return Err(IrError::TypeMismatch {
            func: func.name.clone(),
            block,
            value: operand,
            expected,
            actual,
        });
    }
    Ok(())
}

/// Validates one function in isolation — everything except `Call` sites,
/// which need the whole `Module` to check a callee's signature (see
/// `validate_module`).
pub fn validate_function(func: &Function) -> Result<(), IrError> {
    if func.block(func.entry).is_none() {
        return Err(IrError::UnknownEntryBlock {
            func: func.name.clone(),
            block: func.entry,
        });
    }

    let mut seen_blocks = HashSet::new();
    for block in &func.blocks {
        if !seen_blocks.insert(block.id) {
            return Err(IrError::DuplicateBlock {
                func: func.name.clone(),
                block: block.id,
            });
        }
    }

    let defs = collect_definitions(func)?;
    let preds = func.predecessors();
    let known_blocks: HashSet<BlockId> = func.blocks.iter().map(|b| b.id).collect();

    for block in &func.blocks {
        if block.instructions.is_empty() && block.terminator.is_none() {
            return Err(IrError::EmptyBlock {
                func: func.name.clone(),
                block: block.id,
            });
        }

        for inst in &block.instructions {
            match &inst.kind {
                InstKind::ConstI64(_) => {
                    if inst.ty != Type::I64 {
                        return Err(IrError::TypeMismatch {
                            func: func.name.clone(),
                            block: block.id,
                            value: inst.id,
                            expected: Type::I64,
                            actual: inst.ty,
                        });
                    }
                }
                InstKind::ConstBool(_) => {
                    if inst.ty != Type::Bool {
                        return Err(IrError::TypeMismatch {
                            func: func.name.clone(),
                            block: block.id,
                            value: inst.id,
                            expected: Type::Bool,
                            actual: inst.ty,
                        });
                    }
                }
                InstKind::Bin(op, lhs, rhs) => {
                    check_operand(func, block.id, &defs, *lhs, op.operand_type())?;
                    check_operand(func, block.id, &defs, *rhs, op.operand_type())?;
                    if inst.ty != op.result_type() {
                        return Err(IrError::TypeMismatch {
                            func: func.name.clone(),
                            block: block.id,
                            value: inst.id,
                            expected: op.result_type(),
                            actual: inst.ty,
                        });
                    }
                }
                InstKind::Un(op, operand) => {
                    check_operand(func, block.id, &defs, *operand, op.operand_type())?;
                    if inst.ty != op.result_type() {
                        return Err(IrError::TypeMismatch {
                            func: func.name.clone(),
                            block: block.id,
                            value: inst.id,
                            expected: op.result_type(),
                            actual: inst.ty,
                        });
                    }
                }
                InstKind::Call { args, .. } => {
                    // Callee existence/signature is checked in
                    // validate_module; here we only need every argument
                    // to reference a value that's actually defined.
                    for arg in args {
                        if !defs.contains_key(arg) {
                            return Err(IrError::UndefinedValue {
                                func: func.name.clone(),
                                block: block.id,
                                value: *arg,
                            });
                        }
                    }
                }
                InstKind::Phi(incoming) => {
                    let block_preds = preds.get(&block.id).cloned().unwrap_or_default();
                    if incoming.len() != block_preds.len() {
                        return Err(IrError::PhiArityMismatch {
                            func: func.name.clone(),
                            block: block.id,
                            value: inst.id,
                            expected: block_preds.len(),
                            given: incoming.len(),
                        });
                    }
                    let pred_set: HashSet<BlockId> = block_preds.into_iter().collect();
                    for (from, value) in incoming {
                        if !pred_set.contains(from) {
                            return Err(IrError::PhiUnknownPredecessor {
                                func: func.name.clone(),
                                block: block.id,
                                value: inst.id,
                                named: *from,
                            });
                        }
                        check_operand(func, block.id, &defs, *value, inst.ty)?;
                    }
                }
            }
        }

        match &block.terminator {
            None => {
                return Err(IrError::MissingTerminator {
                    func: func.name.clone(),
                    block: block.id,
                })
            }
            Some(Terminator::Jump(target)) => {
                if !known_blocks.contains(target) {
                    return Err(IrError::UnknownBlock {
                        func: func.name.clone(),
                        block: block.id,
                        target: *target,
                    });
                }
            }
            Some(Terminator::Branch {
                cond,
                then_block,
                else_block,
            }) => {
                check_operand(func, block.id, &defs, *cond, Type::Bool)?;
                for target in [then_block, else_block] {
                    if !known_blocks.contains(target) {
                        return Err(IrError::UnknownBlock {
                            func: func.name.clone(),
                            block: block.id,
                            target: *target,
                        });
                    }
                }
            }
            Some(Terminator::Return(value)) => match (func.ret_ty, value) {
                (None, Some(_)) => {
                    return Err(IrError::UnexpectedReturnValue {
                        func: func.name.clone(),
                        block: block.id,
                    })
                }
                (Some(expected), None) => {
                    return Err(IrError::MissingReturnValue {
                        func: func.name.clone(),
                        block: block.id,
                        expected,
                    })
                }
                (Some(expected), Some(v)) => {
                    check_operand(func, block.id, &defs, *v, expected)?;
                }
                (None, None) => {}
            },
        }
    }

    Ok(())
}

#[cfg(test)]
mod function_tests {
    use super::*;
    use crate::block::Terminator;
    use crate::inst::{BinOp, InstKind, UnOp};
    use crate::test_support::FnBuilder;

    /// fn add(a: i64, b: i64) -> i64 { return a + b; }
    fn build_add() -> Function {
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
        b.finish()
    }

    #[test]
    fn a_simple_straight_line_function_validates() {
        assert_eq!(validate_function(&build_add()), Ok(()));
    }

    #[test]
    fn a_function_with_a_branch_and_matching_phi_validates() {
        // fn abs(x: i64) -> i64 {
        //   if x < 0 { let y = -x; } else { let y = x; }
        //   return y; // phi
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

        assert_eq!(validate_function(&b.finish()), Ok(()));
    }

    #[test]
    fn a_block_with_no_terminator_is_rejected() {
        let mut b = FnBuilder::new("f", vec![], Some(Type::I64));
        let entry = b.new_block();
        b.set_entry(entry);
        b.push(entry, Type::I64, InstKind::ConstI64(1));
        // no terminator set
        assert_eq!(
            validate_function(&b.finish()),
            Err(IrError::MissingTerminator {
                func: "f".into(),
                block: entry
            })
        );
    }

    #[test]
    fn an_undefined_operand_is_rejected() {
        let mut b = FnBuilder::new("f", vec![], Some(Type::I64));
        let entry = b.new_block();
        b.set_entry(entry);
        let ghost = ValueId(99);
        let result = b.push(entry, Type::I64, InstKind::Un(UnOp::Neg, ghost));
        b.terminate(entry, Terminator::Return(Some(result)));
        assert_eq!(
            validate_function(&b.finish()),
            Err(IrError::UndefinedValue {
                func: "f".into(),
                block: entry,
                value: ghost,
            })
        );
    }

    #[test]
    fn a_type_mismatched_operand_is_rejected() {
        // Add expects two I64 operands; feed it a Bool.
        let mut b = FnBuilder::new("f", vec![], Some(Type::I64));
        let entry = b.new_block();
        b.set_entry(entry);
        let flag = b.push(entry, Type::Bool, InstKind::ConstBool(true));
        let one = b.push(entry, Type::I64, InstKind::ConstI64(1));
        let result = b.push(entry, Type::I64, InstKind::Bin(BinOp::Add, flag, one));
        b.terminate(entry, Terminator::Return(Some(result)));
        assert_eq!(
            validate_function(&b.finish()),
            Err(IrError::TypeMismatch {
                func: "f".into(),
                block: entry,
                value: flag,
                expected: Type::I64,
                actual: Type::Bool,
            })
        );
    }

    #[test]
    fn a_phi_with_wrong_incoming_count_is_rejected() {
        let mut b = FnBuilder::new("f", vec![], Some(Type::I64));
        let entry = b.new_block();
        let a = b.new_block();
        let bb = b.new_block();
        let join = b.new_block();
        b.set_entry(entry);

        let cond = b.push(entry, Type::Bool, InstKind::ConstBool(true));
        b.terminate(
            entry,
            Terminator::Branch {
                cond,
                then_block: a,
                else_block: bb,
            },
        );
        let one = b.push(a, Type::I64, InstKind::ConstI64(1));
        b.terminate(a, Terminator::Jump(join));
        b.terminate(bb, Terminator::Jump(join));

        // Only one incoming value given, but `join` has two predecessors.
        let phi = b.push(join, Type::I64, InstKind::Phi(vec![(a, one)]));
        b.terminate(join, Terminator::Return(Some(phi)));

        assert_eq!(
            validate_function(&b.finish()),
            Err(IrError::PhiArityMismatch {
                func: "f".into(),
                block: join,
                value: phi,
                expected: 2,
                given: 1,
            })
        );
    }

    #[test]
    fn jumping_to_an_unknown_block_is_rejected() {
        let mut b = FnBuilder::new("f", vec![], None);
        let entry = b.new_block();
        b.set_entry(entry);
        let ghost_block = BlockId(77);
        b.terminate(entry, Terminator::Jump(ghost_block));
        assert_eq!(
            validate_function(&b.finish()),
            Err(IrError::UnknownBlock {
                func: "f".into(),
                block: entry,
                target: ghost_block,
            })
        );
    }

    #[test]
    fn returning_a_value_from_a_void_function_is_rejected() {
        let mut b = FnBuilder::new("f", vec![], None);
        let entry = b.new_block();
        b.set_entry(entry);
        let one = b.push(entry, Type::I64, InstKind::ConstI64(1));
        b.terminate(entry, Terminator::Return(Some(one)));
        assert_eq!(
            validate_function(&b.finish()),
            Err(IrError::UnexpectedReturnValue {
                func: "f".into(),
                block: entry,
            })
        );
    }

    #[test]
    fn returning_nothing_from_a_non_void_function_is_rejected() {
        let mut b = FnBuilder::new("f", vec![], Some(Type::I64));
        let entry = b.new_block();
        b.set_entry(entry);
        b.terminate(entry, Terminator::Return(None));
        assert_eq!(
            validate_function(&b.finish()),
            Err(IrError::MissingReturnValue {
                func: "f".into(),
                block: entry,
                expected: Type::I64,
            })
        );
    }

    #[test]
    fn a_duplicate_value_id_is_rejected() {
        let mut b = FnBuilder::new("f", vec![], Some(Type::I64));
        let entry = b.new_block();
        b.set_entry(entry);
        let dup = ValueId(0);
        b.push_with_id(entry, dup, Type::I64, InstKind::ConstI64(1));
        b.push_with_id(entry, dup, Type::I64, InstKind::ConstI64(2));
        b.terminate(entry, Terminator::Return(Some(dup)));
        assert_eq!(
            validate_function(&b.finish()),
            Err(IrError::DuplicateValue {
                func: "f".into(),
                value: dup,
            })
        );
    }

    #[test]
    fn an_unknown_entry_block_is_rejected() {
        let mut b = FnBuilder::new("f", vec![], None);
        let real = b.new_block();
        b.terminate(real, Terminator::Return(None));
        b.set_entry(BlockId(42));
        assert_eq!(
            validate_function(&b.finish()),
            Err(IrError::UnknownEntryBlock {
                func: "f".into(),
                block: BlockId(42),
            })
        );
    }
}

/// Validates every function individually, then every `Call` site against
/// the callee's actual signature — the one check that genuinely needs
/// the whole module rather than a single function in isolation.
pub fn validate_module(module: &Module) -> Result<(), IrError> {
    for func in &module.functions {
        validate_function(func)?;
    }

    let defs_by_func: HashMap<&str, HashMap<ValueId, Type>> = module
        .functions
        .iter()
        .map(|f| (f.name.as_str(), collect_definitions(f).unwrap_or_default()))
        .collect();

    for func in &module.functions {
        let defs = &defs_by_func[func.name.as_str()];
        for block in &func.blocks {
            for inst in &block.instructions {
                if let InstKind::Call { func: callee, args } = &inst.kind {
                    let callee_fn = module.function(callee).ok_or(IrError::UnknownCallee {
                        func: func.name.clone(),
                        block: block.id,
                        callee: callee.clone(),
                    })?;
                    if callee_fn.params.len() != args.len() {
                        return Err(IrError::CallArityMismatch {
                            func: func.name.clone(),
                            block: block.id,
                            callee: callee.clone(),
                            expected: callee_fn.params.len(),
                            given: args.len(),
                        });
                    }
                    for (arg, (_, expected_ty)) in args.iter().zip(&callee_fn.params) {
                        check_operand(func, block.id, defs, *arg, *expected_ty)?;
                    }
                    let Some(expected_result) = callee_fn.ret_ty else {
                        return Err(IrError::CallToVoidFunctionUsedAsValue {
                            func: func.name.clone(),
                            block: block.id,
                            callee: callee.clone(),
                        });
                    };
                    if inst.ty != expected_result {
                        return Err(IrError::TypeMismatch {
                            func: func.name.clone(),
                            block: block.id,
                            value: inst.id,
                            expected: expected_result,
                            actual: inst.ty,
                        });
                    }
                }
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod module_tests {
    use super::*;
    use crate::block::Terminator;
    use crate::inst::InstKind;
    use crate::test_support::FnBuilder;

    /// fn callee(x: i64) -> i64 { return x; }
    /// fn caller() -> i64 { return callee(41); }
    fn build_caller_callee() -> Module {
        let mut cb = FnBuilder::new("callee", vec![("x", Type::I64)], Some(Type::I64));
        let cb_entry = cb.new_block();
        cb.set_entry(cb_entry);
        cb.terminate(cb_entry, Terminator::Return(Some(cb.param_value(0))));

        let mut ca = FnBuilder::new("caller", vec![], Some(Type::I64));
        let ca_entry = ca.new_block();
        ca.set_entry(ca_entry);
        let forty_one = ca.push(ca_entry, Type::I64, InstKind::ConstI64(41));
        let result = ca.push(
            ca_entry,
            Type::I64,
            InstKind::Call {
                func: "callee".into(),
                args: vec![forty_one],
            },
        );
        ca.terminate(ca_entry, Terminator::Return(Some(result)));

        Module {
            functions: vec![cb.finish(), ca.finish()],
        }
    }

    #[test]
    fn a_well_formed_call_validates() {
        assert_eq!(validate_module(&build_caller_callee()), Ok(()));
    }

    #[test]
    fn calling_an_unknown_function_is_rejected() {
        let mut b = FnBuilder::new("caller", vec![], Some(Type::I64));
        let entry = b.new_block();
        b.set_entry(entry);
        let result = b.push(
            entry,
            Type::I64,
            InstKind::Call {
                func: "nonexistent".into(),
                args: vec![],
            },
        );
        b.terminate(entry, Terminator::Return(Some(result)));
        let module = Module {
            functions: vec![b.finish()],
        };
        assert_eq!(
            validate_module(&module),
            Err(IrError::UnknownCallee {
                func: "caller".into(),
                block: BlockId(0),
                callee: "nonexistent".into(),
            })
        );
    }

    #[test]
    fn calling_with_the_wrong_argument_count_is_rejected() {
        let mut module = build_caller_callee();
        // Break the call site to pass zero args to a one-arg function.
        if let InstKind::Call { args, .. } = &mut module.functions[1].blocks[0].instructions[1].kind
        {
            args.clear();
        }
        assert_eq!(
            validate_module(&module),
            Err(IrError::CallArityMismatch {
                func: "caller".into(),
                block: BlockId(0),
                callee: "callee".into(),
                expected: 1,
                given: 0,
            })
        );
    }

    #[test]
    fn calling_a_void_function_and_using_the_result_is_rejected() {
        let mut cb = FnBuilder::new("log", vec![], None);
        let cb_entry = cb.new_block();
        cb.set_entry(cb_entry);
        cb.terminate(cb_entry, Terminator::Return(None));

        let mut ca = FnBuilder::new("caller", vec![], Some(Type::I64));
        let ca_entry = ca.new_block();
        ca.set_entry(ca_entry);
        let result = ca.push(
            ca_entry,
            Type::I64,
            InstKind::Call {
                func: "log".into(),
                args: vec![],
            },
        );
        ca.terminate(ca_entry, Terminator::Return(Some(result)));

        let module = Module {
            functions: vec![cb.finish(), ca.finish()],
        };
        assert_eq!(
            validate_module(&module),
            Err(IrError::CallToVoidFunctionUsedAsValue {
                func: "caller".into(),
                block: BlockId(0),
                callee: "log".into(),
            })
        );
    }
}
