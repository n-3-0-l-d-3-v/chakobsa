//! Register allocation: greedy graph coloring over a block-granularity
//! interference graph (see `liveness`'s precision trade-off note),
//! bounded by mentat's `isa::NUM_REGISTERS`, spilling to memory when a
//! value can't be colored. Function parameters are pre-colored to fixed
//! registers (`ValueId(i)` -> register `i`) to match the calling
//! convention `docs/design/decisions/ADR-004-codegen-and-calling-convention.md`
//! documents: a callee reads its arguments from fixed registers at
//! entry, so a parameter's register assignment isn't a free choice.

use std::collections::{HashMap, HashSet};

use ir::{Function, ValueId};

use crate::liveness::{self, live_within_block};

/// Reserved for spilled-value Load/Store addressing. Never written by
/// any code this crate generates, so it stays at mentat's registers'
/// zero-initialized start-of-program value for the whole run — no
/// explicit "load zero" instruction is needed. A spill is then just
/// `Load/Store(ZERO_REG, absolute_address)`, absolute addressing with
/// no separate frame pointer or stack.
pub const ZERO_REG: u8 = 30;
/// Reserved for a function's return value: the callee writes its result
/// here immediately before `Ret`; the caller reads it immediately after
/// a `Call` returns.
pub const RET_REG: u8 = 31;
/// Reserved for reloading up to two spilled operands (or staging a
/// spilled result) around a single instruction.
pub const SCRATCH_1: u8 = 29;
pub const SCRATCH_2: u8 = 28;
/// Reserved for the software call stack `lower` pushes/pops caller-live
/// values through around every `Call` (mentat's own `Call`/`Ret` only
/// track a return-address stack of block indices — no value storage —
/// so anything a caller needs to survive a call it might recurse
/// through has to be saved somewhere the callee's own execution, however
/// deep, can't collide with).
pub const STACK_PTR: u8 = 27;
/// General-purpose registers available to the allocator at all:
/// `0..NUM_GP_REGS`. Of these, `0..reserved_arg_registers` (see
/// `allocate`) are further set aside module-wide for argument passing.
pub const NUM_GP_REGS: u8 = 27;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum RegAllocError {
    #[error("function '{func}' has {count} parameters, more than the {max} the calling convention can pass in registers")]
    TooManyParameters { func: String, count: usize, max: u8 },
}

#[derive(Debug, Default)]
pub struct Allocation {
    pub registers: HashMap<ValueId, u8>,
    /// Absolute memory address for each spilled value.
    pub spills: HashMap<ValueId, u64>,
}

impl Allocation {
    pub fn is_spilled(&self, v: ValueId) -> bool {
        self.spills.contains_key(&v)
    }
}

/// Allocates registers for every value `func` defines (its parameters
/// included). Spilled values are assigned successive 8-byte-aligned
/// addresses starting at `spill_base_addr`.
///
/// `reserved_arg_registers` (computed once, module-wide, as the largest
/// parameter count of any function being compiled) excludes registers
/// `0..reserved_arg_registers` from every *non-parameter* value's
/// candidate set. Without this, a value with no relation to any call
/// could still legally be colored into e.g. register 0 by pure
/// interference-graph luck and survive live across a `Call` — which
/// physically overwrites registers `0..callee.params.len()` with the
/// callee's arguments no matter what the caller was keeping there (a
/// classic caller-saved-register hazard). Reserving the whole
/// module's argument-register range for arguments/parameters only,
/// rather than trying to make liveness call-site-aware, sidesteps the
/// hazard entirely at the cost of a few general-purpose registers system-
/// wide — a deliberate, documented trade-off (see
/// `docs/design/decisions/ADR-004-codegen-and-calling-convention.md`),
/// not an oversight.
///
/// **Known limitation, not engineered around**: spill addresses are a
/// fixed, static range per function, not relative to a call-stack frame.
/// A recursive function whose calls are simultaneously live and which
/// actually needs to spill would have each active call corrupt the
/// others' spill slots. None of this ticket's test programs spill
/// (mentat's registers comfortably cover every test function's live-
/// value count), so this is a real, currently-unexercised gap, tracked
/// honestly rather than silently assumed safe — see ADR-004.
pub fn allocate(
    func: &Function,
    spill_base_addr: u64,
    reserved_arg_registers: u8,
) -> Result<Allocation, RegAllocError> {
    if func.params.len() > NUM_GP_REGS as usize {
        return Err(RegAllocError::TooManyParameters {
            func: func.name.clone(),
            count: func.params.len(),
            max: NUM_GP_REGS,
        });
    }

    let liveness = liveness::analyze(func);

    // Interference: two values conflict if they're both live at any
    // point within the same block (see liveness's block-granularity
    // trade-off).
    let mut interferes: HashMap<ValueId, HashSet<ValueId>> = HashMap::new();
    let mut all_values: HashSet<ValueId> = HashSet::new();
    for (i, _) in func.params.iter().enumerate() {
        all_values.insert(ValueId(i as u32));
    }
    for block in &func.blocks {
        for inst in &block.instructions {
            all_values.insert(inst.id);
        }
    }
    for block in &func.blocks {
        let live = live_within_block(func, block.id, &liveness);
        for &a in &live {
            for &b in &live {
                if a != b {
                    interferes.entry(a).or_default().insert(b);
                }
            }
        }
    }

    let mut allocation = Allocation::default();

    // Parameters are pre-colored: register i for param i, non-negotiable
    // (mentat's Call has no argument-passing mechanism of its own — a
    // callee simply reads whatever is already in its parameter
    // registers when it starts executing).
    for (i, _) in func.params.iter().enumerate() {
        allocation.registers.insert(ValueId(i as u32), i as u8);
    }

    // Greedily color every other value, in ascending ValueId order for
    // determinism. A value with no available register spills.
    let mut others: Vec<ValueId> = all_values
        .iter()
        .copied()
        .filter(|v| !allocation.registers.contains_key(v))
        .collect();
    others.sort_by_key(|v| v.0);

    let mut next_spill_addr = spill_base_addr;
    for value in others {
        let used_by_neighbors: HashSet<u8> = interferes
            .get(&value)
            .into_iter()
            .flatten()
            .filter_map(|n| allocation.registers.get(n).copied())
            .collect();
        let chosen = (reserved_arg_registers..NUM_GP_REGS).find(|r| !used_by_neighbors.contains(r));
        match chosen {
            Some(reg) => {
                allocation.registers.insert(value, reg);
            }
            None => {
                allocation.spills.insert(value, next_spill_addr);
                next_spill_addr += 8;
            }
        }
    }

    Ok(allocation)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ir::{BasicBlock, BinOp, BlockId, Function, InstKind, Instruction, Terminator, Type};

    /// fn add(a: i64, b: i64) -> i64 { return a + b; }
    fn build_add() -> Function {
        let sum = ValueId(2);
        Function {
            name: "add".to_string(),
            params: vec![("a".to_string(), Type::I64), ("b".to_string(), Type::I64)],
            ret_ty: Some(Type::I64),
            blocks: vec![BasicBlock {
                id: BlockId(0),
                instructions: vec![Instruction {
                    id: sum,
                    ty: Type::I64,
                    kind: InstKind::Bin(BinOp::Add, ValueId(0), ValueId(1)),
                }],
                terminator: Some(Terminator::Return(Some(sum))),
            }],
            entry: BlockId(0),
        }
    }

    #[test]
    fn parameters_are_pre_colored_to_their_positional_register() {
        let func = build_add();
        let alloc = allocate(&func, 0, 0).unwrap();
        assert_eq!(alloc.registers[&ValueId(0)], 0);
        assert_eq!(alloc.registers[&ValueId(1)], 1);
    }

    #[test]
    fn a_value_with_no_interference_gets_a_register_not_a_spill() {
        let func = build_add();
        let alloc = allocate(&func, 0, 0).unwrap();
        assert!(!alloc.is_spilled(ValueId(2)));
        assert!(alloc.registers.contains_key(&ValueId(2)));
    }

    #[test]
    fn reserved_arg_registers_are_never_given_to_non_parameter_values() {
        let func = build_add();
        // Pretend some other function in the module takes 5 parameters,
        // reserving registers 0..4 module-wide.
        let alloc = allocate(&func, 0, 5).unwrap();
        // add's own params still get their fixed positional registers
        // (0 and 1) — the reservation only excludes OTHER values.
        assert_eq!(alloc.registers[&ValueId(0)], 0);
        assert_eq!(alloc.registers[&ValueId(1)], 1);
        // But `sum` (not a parameter) must land outside 0..5.
        assert!(alloc.registers[&ValueId(2)] >= 5);
    }

    #[test]
    fn more_concurrently_live_values_than_registers_forces_a_spill() {
        // A block that keeps 40 distinct values simultaneously alive
        // (each added to a running total) — comfortably more than
        // NUM_GP_REGS, so at least one must spill.
        let mut instructions = Vec::new();
        let mut running = ValueId(0); // param 0 is the running total's start
        let mut kept = Vec::new();
        for i in 0..40u32 {
            let id = ValueId(10 + i);
            instructions.push(Instruction {
                id,
                ty: Type::I64,
                kind: InstKind::ConstI64(i as i64),
            });
            kept.push(id);
        }
        // Use every one of them at the very end, so all 40 are
        // simultaneously live going into that final instruction.
        let mut acc = kept[0];
        for &k in &kept[1..] {
            let next = ValueId(1000 + k.0);
            instructions.push(Instruction {
                id: next,
                ty: Type::I64,
                kind: InstKind::Bin(BinOp::Add, acc, k),
            });
            acc = next;
        }
        let _ = &mut running;

        let func = Function {
            name: "many_live".to_string(),
            params: vec![],
            ret_ty: Some(Type::I64),
            blocks: vec![BasicBlock {
                id: BlockId(0),
                instructions,
                terminator: Some(Terminator::Return(Some(acc))),
            }],
            entry: BlockId(0),
        };

        let alloc = allocate(&func, 0, 0).unwrap();
        assert!(
            !alloc.spills.is_empty(),
            "40 concurrently-referenced values must exceed {NUM_GP_REGS} registers and force at least one spill"
        );
    }

    #[test]
    fn too_many_parameters_is_a_typed_error() {
        let params: Vec<(String, Type)> = (0..(NUM_GP_REGS as usize + 1))
            .map(|i| (format!("p{i}"), Type::I64))
            .collect();
        let func = Function::new("f", params, Some(Type::I64));
        let result = allocate(&func, 0, 0);
        assert!(matches!(
            result,
            Err(RegAllocError::TooManyParameters { .. })
        ));
    }
}
