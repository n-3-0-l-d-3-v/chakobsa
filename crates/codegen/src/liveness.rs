//! Block-level liveness for register allocation: for each `ir::Function`,
//! which `ValueId`s are live going into and out of each block.
//!
//! **Deliberate precision trade-off**: liveness is computed at block
//! granularity (a value live *anywhere* in a block counts as live for
//! the *whole* block), not instruction granularity. This is always
//! conservative — it can never under-count liveness, so it can never
//! cause a correctness bug (a real interference gets missed) — but it
//! can over-count within a block, causing more register pressure and
//! spills than a precise interval-based allocator would. Given mentat's
//! generous 32 physical registers and this language's deliberately small
//! v1 surface area (`docs/design/LANGUAGE.md`: no arrays/structs, so
//! function bodies stay small), this is judged an acceptable, real
//! scope choice for a first working allocator — not an oversight. See
//! `docs/design/decisions/ADR-004-codegen-and-calling-convention.md`.

use std::collections::{HashMap, HashSet};

use ir::{BlockId, Function, InstKind, Terminator, ValueId};

/// `used[b]`: values read in block `b` before any (re)definition in `b`
/// reaches them — irrelevant at block granularity since we only care
/// about "read anywhere in this block," so this is simply every value
/// referenced by any instruction or terminator in `b`, defined in `b`
/// or not.
///
/// **A `Phi`'s incoming values are deliberately excluded here.** A phi
/// operand for predecessor `P` is only ever actually read at the end of
/// `P` — not "somewhere in the block containing the phi." Counting it as
/// a use of the phi's own block would make it look live going *into*
/// that block, which (through a loop's back edge) can leak backward
/// through the whole cycle and beyond, since the phi's block is often
/// its own eventual predecessor. `analyze` attributes each phi operand
/// to the correct specific predecessor edge instead — see its own
/// comment for the real, caught-by-testing bug this replaced.
fn used_and_defined_in_block(
    func: &Function,
    block: BlockId,
) -> (HashSet<ValueId>, HashSet<ValueId>) {
    let mut used = HashSet::new();
    let mut defined: HashSet<ValueId> = HashSet::new();
    let b = func.block(block).expect("block id from this function");

    // A value referenced after this block has already defined it (e.g.
    // `one = 1; acc_next = acc_phi + one;`) is a purely internal,
    // block-local value — never live *into* the block from outside, so
    // it must not join `used`, only the "upward-exposed" case (a
    // reference to something not yet defined so far in this block)
    // counts. Getting this wrong lets a value with a real, short,
    // block-local lifetime look like it's live at the block's entry —
    // and from there, through a loop's back edge, leak indefinitely far
    // outside where it's ever actually needed (caught by this module's
    // own tests: `one`, defined and consumed entirely within a loop
    // body, was showing up as live all the way back at the function's
    // entry block before this fix).
    fn note_use(used: &mut HashSet<ValueId>, defined: &HashSet<ValueId>, v: ValueId) {
        if !defined.contains(&v) {
            used.insert(v);
        }
    }

    for inst in &b.instructions {
        match &inst.kind {
            InstKind::ConstI64(_) | InstKind::ConstBool(_) => {}
            InstKind::Bin(_, a, c) => {
                note_use(&mut used, &defined, *a);
                note_use(&mut used, &defined, *c);
            }
            InstKind::Un(_, a) => {
                note_use(&mut used, &defined, *a);
            }
            InstKind::Call { args, .. } => {
                for a in args {
                    note_use(&mut used, &defined, *a);
                }
            }
            InstKind::Phi(_) => {} // see this function's doc comment
        }
        defined.insert(inst.id);
    }
    match &b.terminator {
        Some(Terminator::Branch { cond, .. }) => {
            note_use(&mut used, &defined, *cond);
        }
        Some(Terminator::Return(Some(v))) => {
            note_use(&mut used, &defined, *v);
        }
        _ => {}
    }
    (used, defined)
}

/// Every `(value, defining_phi_block)` a phi in `block` receives from
/// predecessor `from` — the set `analyze` adds directly to
/// `live_out[from]`, since that's the one place each is actually used.
fn phi_operands_from(func: &Function, block: BlockId, from: BlockId) -> Vec<ValueId> {
    func.block(block)
        .into_iter()
        .flat_map(|b| &b.instructions)
        .filter_map(|inst| match &inst.kind {
            InstKind::Phi(incoming) => incoming.iter().find(|(p, _)| *p == from).map(|(_, v)| *v),
            _ => None,
        })
        .collect()
}

/// `live_in`/`live_out` per block, computed by the standard iterative
/// backward dataflow: `live_out[b] = union of live_in[s] for successors
/// s`; `live_in[b] = used[b] ∪ (live_out[b] - defined[b])`. Iterates to
/// a fixed point, which always terminates (each set only ever grows,
/// bounded by the finite set of values in the function).
pub struct Liveness {
    pub live_in: HashMap<BlockId, HashSet<ValueId>>,
    pub live_out: HashMap<BlockId, HashSet<ValueId>>,
}

pub fn analyze(func: &Function) -> Liveness {
    let mut used = HashMap::new();
    let mut defined = HashMap::new();
    let mut live_in: HashMap<BlockId, HashSet<ValueId>> = HashMap::new();
    let mut live_out: HashMap<BlockId, HashSet<ValueId>> = HashMap::new();

    for block in &func.blocks {
        let (u, d) = used_and_defined_in_block(func, block.id);
        used.insert(block.id, u);
        defined.insert(block.id, d);
        live_in.insert(block.id, HashSet::new());
        live_out.insert(block.id, HashSet::new());
    }

    // Successors, straight from each block's own terminator.
    let mut succs: HashMap<BlockId, Vec<BlockId>> = HashMap::new();
    for block in &func.blocks {
        if let Some(term) = &block.terminator {
            succs.insert(block.id, term.successors());
        }
    }

    let mut changed = true;
    while changed {
        changed = false;
        for block in &func.blocks {
            let mut out = HashSet::new();
            for succ in succs.get(&block.id).into_iter().flatten() {
                out.extend(live_in[succ].iter().copied());
                // A phi in `succ` reads its `block`-specific operand
                // exactly at the end of `block` — always live-out of
                // `block`, whether or not it happens to also be part of
                // succ's general live_in (see used_and_defined_in_block's
                // doc comment for why phi operands can't just be folded
                // into succ's own `used` set instead).
                out.extend(phi_operands_from(func, *succ, block.id));
            }
            if out != live_out[&block.id] {
                live_out.insert(block.id, out.clone());
                changed = true;
            }

            let mut new_in = used[&block.id].clone();
            for v in &out {
                if !defined[&block.id].contains(v) {
                    new_in.insert(*v);
                }
            }
            if new_in != live_in[&block.id] {
                live_in.insert(block.id, new_in);
                changed = true;
            }
        }
    }

    Liveness { live_in, live_out }
}

/// Every value live at any point within `block` — used, defined, or
/// simply passing through (live-in and not yet dead) — the set two
/// values must NOT share a register if they're both members (see
/// `regalloc`'s interference graph, which conflicts values sharing a
/// live block, matching this function's block-granularity precision
/// trade-off exactly).
pub fn live_within_block(func: &Function, block: BlockId, liveness: &Liveness) -> HashSet<ValueId> {
    let (used, defined) = used_and_defined_in_block(func, block);
    let mut live = liveness.live_in[&block].clone();
    live.extend(used);
    live.extend(defined);
    live.extend(liveness.live_out[&block].iter().copied());
    live
}

#[cfg(test)]
mod tests {
    use super::*;
    use ir::{BasicBlock, BinOp, Function, Instruction, Terminator, Type};

    /// fn f(n: i64) -> i64 {
    ///   let acc = 0;                  // b0
    ///   while acc < n { acc = acc+1; } // b1 (header), b2 (body)
    ///   return acc;                   // b3
    /// }
    /// A loop-carried phi in the header, read in both the header (cond)
    /// and the body (increment) — the case block-level liveness has to
    /// get right across the loop's back edge.
    fn build_loop_fn() -> Function {
        let n = ValueId(0);
        let zero = ValueId(1);
        let acc_phi = ValueId(2);
        let cond = ValueId(3);
        let acc_next = ValueId(4);
        let one = ValueId(5);

        let b0 = BlockId(0);
        let header = BlockId(1);
        let body = BlockId(2);
        let exit = BlockId(3);

        let block0 = BasicBlock {
            id: b0,
            instructions: vec![Instruction {
                id: zero,
                ty: Type::I64,
                kind: InstKind::ConstI64(0),
            }],
            terminator: Some(Terminator::Jump(header)),
        };
        let header_block = BasicBlock {
            id: header,
            instructions: vec![
                Instruction {
                    id: acc_phi,
                    ty: Type::I64,
                    kind: InstKind::Phi(vec![(b0, zero), (body, acc_next)]),
                },
                Instruction {
                    id: cond,
                    ty: Type::Bool,
                    kind: InstKind::Bin(BinOp::CmpLt, acc_phi, n),
                },
            ],
            terminator: Some(Terminator::Branch {
                cond,
                then_block: body,
                else_block: exit,
            }),
        };
        let body_block = BasicBlock {
            id: body,
            instructions: vec![
                Instruction {
                    id: one,
                    ty: Type::I64,
                    kind: InstKind::ConstI64(1),
                },
                Instruction {
                    id: acc_next,
                    ty: Type::I64,
                    kind: InstKind::Bin(BinOp::Add, acc_phi, one),
                },
            ],
            terminator: Some(Terminator::Jump(header)),
        };
        let exit_block = BasicBlock {
            id: exit,
            instructions: vec![],
            terminator: Some(Terminator::Return(Some(acc_phi))),
        };

        Function {
            name: "f".to_string(),
            params: vec![("n".to_string(), Type::I64)],
            ret_ty: Some(Type::I64),
            blocks: vec![block0, header_block, body_block, exit_block],
            entry: b0,
        }
    }

    #[test]
    fn the_functions_own_parameter_is_live_everywhere_it_could_still_be_read() {
        let func = build_loop_fn();
        let liveness = analyze(&func);
        // `n` (ValueId(0)) is read only in the header's comparison, but
        // it must be live all the way from entry through the header —
        // otherwise the allocator could reuse its register too early.
        assert!(liveness.live_out[&BlockId(0)].contains(&ValueId(0)));
        assert!(liveness.live_in[&BlockId(1)].contains(&ValueId(0)));
    }

    #[test]
    fn the_loop_carried_phi_is_live_across_the_back_edge() {
        let func = build_loop_fn();
        let liveness = analyze(&func);
        let acc_phi = ValueId(2);
        // The body must see acc_phi as live-in (it reads it to compute
        // acc_next) and the header must see it live-out (the back edge
        // carries it back around).
        assert!(liveness.live_in[&BlockId(2)].contains(&acc_phi));
        assert!(liveness.live_out[&BlockId(1)].contains(&acc_phi));
    }

    #[test]
    fn a_value_dead_after_its_own_block_is_not_live_out() {
        let func = build_loop_fn();
        let liveness = analyze(&func);
        let one = ValueId(5); // only ever used within the body block itself
        assert!(!liveness.live_out[&BlockId(2)].contains(&one));
    }

    #[test]
    fn live_within_block_includes_both_used_and_passed_through_values() {
        let func = build_loop_fn();
        let liveness = analyze(&func);
        let live = live_within_block(&func, BlockId(1), &liveness);
        assert!(live.contains(&ValueId(0))); // n: passed through (live-in and live-out)
        assert!(live.contains(&ValueId(2))); // acc_phi: defined here
        assert!(live.contains(&ValueId(3))); // cond: defined here
    }
}
