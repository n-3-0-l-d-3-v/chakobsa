//! Lowers one `ir::Function` to a sequence of "pending" mentat blocks —
//! real instructions, but with control-flow targets left as `ir::BlockId`
//! (intra-function) or a callee name (`Call`), resolved to absolute
//! mentat block indices only once the whole module's layout is known
//! (see `lib.rs`'s `compile_module`, which needs every function's block
//! count before it can assign anyone an absolute address).
//!
//! Three things this pass has to get right that a straightforward
//! "just translate each instruction" pass would miss — see
//! `docs/design/decisions/ADR-004-codegen-and-calling-convention.md`
//! for the full reasoning:
//!
//! 1. **mentat's `Jz`/`Jnz` name only one target**; the other is
//!    whichever block is physically next. Rather than controlling block
//!    layout globally to exploit that, every `ir::Branch` lowers to two
//!    physical mentat blocks: one ending in `Jz` to the else-target,
//!    immediately followed by a one-instruction "thunk" block that
//!    unconditionally jumps to the then-target. Correct regardless of
//!    how anything else is laid out.
//! 2. **mentat's `Call` is a terminator** (control resumes at whatever
//!    is physically next after it, once `Ret` fires) but `ir::Call` is
//!    an ordinary instruction that can sit in the middle of a block. A
//!    block containing a `Call` therefore always splits in two at that
//!    point.
//! 3. **Multi-value moves (`Call` arguments, `Phi` elimination) are
//!    parallel, not sequential.** Moving call arguments or resolving a
//!    block's incoming phis one `Mov` at a time can clobber a value
//!    before it's read if two of the moves happen to alias registers
//!    (e.g. swapping two parameters into a call: `g(b, a)`). Every
//!    multi-value move in this module goes through `parallel_move`,
//!    which stages every source into scratch memory before writing any
//!    destination — always correct, independent of aliasing.

use ir::{BinOp, BlockId, Function, InstKind, Terminator, UnOp, ValueId};
use isa::{Instruction, Opcode};
use std::collections::HashMap;

use crate::liveness::{self, live_within_block, Liveness};
use crate::regalloc::{Allocation, RET_REG, SCRATCH_1, SCRATCH_2, STACK_PTR, ZERO_REG};
use crate::CodegenError;

#[derive(Clone, Copy)]
enum Loc {
    Reg(u8),
    Spill(u64),
}

pub enum PendingTerm {
    Jmp(BlockId),
    /// Jump to `target` if the value in `cond_reg` is zero (false);
    /// otherwise fall through to the next pending block.
    Jz {
        cond_reg: u8,
        target: BlockId,
    },
    Call {
        callee: String,
    },
    Ret,
}

pub struct PendingBlock {
    pub instructions: Vec<Instruction>,
    pub term: PendingTerm,
}

impl PendingBlock {
    fn push(&mut self, ins: Instruction) {
        self.instructions.push(ins);
    }
}

struct Lowerer<'a> {
    func: &'a Function,
    alloc: &'a Allocation,
    liveness: Liveness,
    pending: Vec<PendingBlock>,
    local_index: HashMap<BlockId, usize>,
    next_temp_addr: u64,
    region_end: u64,
}

impl<'a> Lowerer<'a> {
    fn loc_of(&self, v: ValueId) -> Loc {
        if let Some(&r) = self.alloc.registers.get(&v) {
            Loc::Reg(r)
        } else {
            Loc::Spill(self.alloc.spills[&v])
        }
    }

    fn addr_i32(&self, addr: u64) -> Result<i32, CodegenError> {
        i32::try_from(addr).map_err(|_| CodegenError::OutOfSpillSpace {
            func: self.func.name.clone(),
        })
    }

    /// Returns a register holding `v`'s current value — its own
    /// register if it has one, otherwise `scratch` after emitting a
    /// reload `Load`.
    fn read_into(
        &mut self,
        blk: &mut PendingBlock,
        v: ValueId,
        scratch: u8,
    ) -> Result<u8, CodegenError> {
        match self.loc_of(v) {
            Loc::Reg(r) => Ok(r),
            Loc::Spill(addr) => {
                blk.push(Instruction::new(
                    Opcode::Load,
                    scratch,
                    ZERO_REG,
                    0,
                    self.addr_i32(addr)?,
                ));
                Ok(scratch)
            }
        }
    }

    /// The register an instruction defining `v` should write its result
    /// into directly — `v`'s own register, or `scratch` if `v` is
    /// spilled (the caller must then call `finish_write`).
    fn write_target(&self, v: ValueId, scratch: u8) -> u8 {
        match self.loc_of(v) {
            Loc::Reg(r) => r,
            Loc::Spill(_) => scratch,
        }
    }

    fn finish_write(
        &mut self,
        blk: &mut PendingBlock,
        v: ValueId,
        reg_used: u8,
    ) -> Result<(), CodegenError> {
        if let Loc::Spill(addr) = self.loc_of(v) {
            blk.push(Instruction::new(
                Opcode::Store,
                ZERO_REG,
                reg_used,
                0,
                self.addr_i32(addr)?,
            ));
        }
        Ok(())
    }

    fn alloc_temp_slots(&mut self, count: usize) -> Result<u64, CodegenError> {
        let base = self.next_temp_addr;
        self.next_temp_addr += (count as u64) * 8;
        if self.next_temp_addr > self.region_end {
            return Err(CodegenError::OutOfSpillSpace {
                func: self.func.name.clone(),
            });
        }
        Ok(base)
    }

    /// Moves every `(source, destination)` pair in `moves` "in parallel"
    /// — every source is staged into scratch memory before any
    /// destination is written, so the pairs can alias registers however
    /// they like (a swap included) without one clobbering another's
    /// still-unread source. See this module's doc comment, point 3.
    fn parallel_move(
        &mut self,
        blk: &mut PendingBlock,
        moves: &[(Loc, Loc)],
    ) -> Result<(), CodegenError> {
        if moves.is_empty() {
            return Ok(());
        }
        let temp_base = self.alloc_temp_slots(moves.len())?;
        for (i, (src, _)) in moves.iter().enumerate() {
            let reg = match src {
                Loc::Reg(r) => *r,
                Loc::Spill(addr) => {
                    blk.push(Instruction::new(
                        Opcode::Load,
                        SCRATCH_1,
                        ZERO_REG,
                        0,
                        self.addr_i32(*addr)?,
                    ));
                    SCRATCH_1
                }
            };
            let temp_addr = self.addr_i32(temp_base + (i as u64) * 8)?;
            blk.push(Instruction::new(Opcode::Store, ZERO_REG, reg, 0, temp_addr));
        }
        for (i, (_, dst)) in moves.iter().enumerate() {
            let temp_addr = self.addr_i32(temp_base + (i as u64) * 8)?;
            match dst {
                Loc::Reg(r) => blk.push(Instruction::new(Opcode::Load, *r, ZERO_REG, 0, temp_addr)),
                Loc::Spill(addr) => {
                    blk.push(Instruction::new(
                        Opcode::Load,
                        SCRATCH_1,
                        ZERO_REG,
                        0,
                        temp_addr,
                    ));
                    blk.push(Instruction::new(
                        Opcode::Store,
                        ZERO_REG,
                        SCRATCH_1,
                        0,
                        self.addr_i32(*addr)?,
                    ));
                }
            }
        }
        Ok(())
    }

    /// Every `(incoming value, phi's own location)` pair a jump from
    /// `from` to `to` must resolve, staged as one parallel move so two
    /// loop-carried phis can't clobber each other.
    fn insert_phi_copies(
        &mut self,
        blk: &mut PendingBlock,
        from: BlockId,
        to: BlockId,
    ) -> Result<(), CodegenError> {
        let target = self
            .func
            .block(to)
            .expect("validated IR: branch target exists");
        let mut moves = Vec::new();
        for inst in &target.instructions {
            if let InstKind::Phi(incoming) = &inst.kind {
                let (_, v) = incoming
                    .iter()
                    .find(|(b, _)| *b == from)
                    .expect("validated IR: phi covers every actual predecessor");
                moves.push((self.loc_of(*v), self.loc_of(inst.id)));
            }
        }
        self.parallel_move(blk, &moves)
    }

    fn lower(mut self) -> Result<(Vec<PendingBlock>, HashMap<BlockId, usize>), CodegenError> {
        for block in &self.func.blocks {
            self.local_index.insert(block.id, self.pending.len());
            let mut cur = PendingBlock {
                instructions: Vec::new(),
                term: PendingTerm::Ret, // placeholder, always overwritten below
            };

            for inst in &block.instructions {
                match &inst.kind {
                    InstKind::Phi(_) => {} // no code; resolved at each predecessor's edge
                    InstKind::ConstI64(n) => {
                        let imm =
                            i32::try_from(*n).map_err(|_| CodegenError::ConstantTooLarge {
                                func: self.func.name.clone(),
                                value: *n,
                            })?;
                        let dst = self.write_target(inst.id, SCRATCH_1);
                        cur.push(Instruction::new(Opcode::LoadI, dst, 0, 0, imm));
                        self.finish_write(&mut cur, inst.id, dst)?;
                    }
                    InstKind::ConstBool(b) => {
                        let dst = self.write_target(inst.id, SCRATCH_1);
                        cur.push(Instruction::new(Opcode::LoadI, dst, 0, 0, *b as i32));
                        self.finish_write(&mut cur, inst.id, dst)?;
                    }
                    InstKind::Bin(op, a, b) => {
                        let ra = self.read_into(&mut cur, *a, SCRATCH_1)?;
                        let rb = self.read_into(&mut cur, *b, SCRATCH_2)?;
                        let dst = self.write_target(inst.id, SCRATCH_1);
                        cur.push(Instruction::new(binop_opcode(*op), dst, ra, rb, 0));
                        self.finish_write(&mut cur, inst.id, dst)?;
                    }
                    InstKind::Un(UnOp::Neg, a) => {
                        // mentat has no dedicated negate opcode: 0 - a,
                        // using the always-zero ZERO_REG as the literal.
                        let ra = self.read_into(&mut cur, *a, SCRATCH_1)?;
                        let dst = self.write_target(inst.id, SCRATCH_1);
                        cur.push(Instruction::new(Opcode::Sub, dst, ZERO_REG, ra, 0));
                        self.finish_write(&mut cur, inst.id, dst)?;
                    }
                    InstKind::Un(UnOp::Not, a) => {
                        // mentat's `Not` is bitwise (!1 == -2), not
                        // logical negation. Our booleans are always 0/1,
                        // so `x == 0` gives exactly logical `not x`.
                        let ra = self.read_into(&mut cur, *a, SCRATCH_1)?;
                        let dst = self.write_target(inst.id, SCRATCH_1);
                        cur.push(Instruction::new(Opcode::CmpEq, dst, ra, ZERO_REG, 0));
                        self.finish_write(&mut cur, inst.id, dst)?;
                    }
                    InstKind::Call { args, .. } => {
                        // Every register-allocated value live anywhere
                        // in this block, other than the call's own
                        // not-yet-defined result, might still be needed
                        // after this call returns — and mentat's Call/
                        // Ret only track a return-address stack, not
                        // register contents, so if the callee (however
                        // deep its own calls go, including recursively
                        // back into this same function) reuses any of
                        // these registers, the caller's value is gone
                        // unless we save it ourselves. See this module's
                        // doc comment and ADR-004.
                        let live = live_within_block(self.func, block.id, &self.liveness);
                        let mut save_regs: Vec<(ValueId, u8)> = live
                            .iter()
                            .filter(|&&v| v != inst.id)
                            .filter_map(|&v| match self.loc_of(v) {
                                Loc::Reg(r) => Some((v, r)),
                                Loc::Spill(_) => None, // already safe in its own memory
                            })
                            .collect();
                        save_regs.sort_by_key(|(v, _)| v.0);
                        save_regs.dedup_by_key(|(v, _)| *v);

                        let frame_bytes = (save_regs.len() as u64) * 8;
                        for (i, &(_, reg)) in save_regs.iter().enumerate() {
                            cur.push(Instruction::new(
                                Opcode::Store,
                                STACK_PTR,
                                reg,
                                0,
                                self.addr_i32((i as u64) * 8)?,
                            ));
                        }
                        if frame_bytes > 0 {
                            cur.push(Instruction::new(
                                Opcode::LoadI,
                                SCRATCH_1,
                                0,
                                0,
                                self.addr_i32(frame_bytes)?,
                            ));
                            cur.push(Instruction::new(
                                Opcode::Add,
                                STACK_PTR,
                                STACK_PTR,
                                SCRATCH_1,
                                0,
                            ));
                        }

                        let moves: Vec<(Loc, Loc)> = args
                            .iter()
                            .enumerate()
                            .map(|(i, a)| (self.loc_of(*a), Loc::Reg(i as u8)))
                            .collect();
                        self.parallel_move(&mut cur, &moves)?;

                        let callee = match &inst.kind {
                            InstKind::Call { func, .. } => func.clone(),
                            _ => unreachable!(),
                        };
                        cur.term = PendingTerm::Call { callee };
                        self.pending.push(cur);

                        cur = PendingBlock {
                            instructions: Vec::new(),
                            term: PendingTerm::Ret, // placeholder
                        };

                        if frame_bytes > 0 {
                            cur.push(Instruction::new(
                                Opcode::LoadI,
                                SCRATCH_1,
                                0,
                                0,
                                self.addr_i32(frame_bytes)?,
                            ));
                            cur.push(Instruction::new(
                                Opcode::Sub,
                                STACK_PTR,
                                STACK_PTR,
                                SCRATCH_1,
                                0,
                            ));
                        }
                        for (i, &(_, reg)) in save_regs.iter().enumerate() {
                            cur.push(Instruction::new(
                                Opcode::Load,
                                reg,
                                STACK_PTR,
                                0,
                                self.addr_i32((i as u64) * 8)?,
                            ));
                        }

                        let dst = self.write_target(inst.id, SCRATCH_1);
                        if dst != RET_REG {
                            cur.push(Instruction::new(Opcode::Mov, dst, RET_REG, 0, 0));
                        }
                        self.finish_write(&mut cur, inst.id, dst)?;
                    }
                }
            }

            match block
                .terminator
                .as_ref()
                .expect("validated IR: every block has a terminator")
            {
                Terminator::Jump(target) => {
                    self.insert_phi_copies(&mut cur, block.id, *target)?;
                    cur.term = PendingTerm::Jmp(*target);
                    self.pending.push(cur);
                }
                Terminator::Branch {
                    cond,
                    then_block,
                    else_block,
                } => {
                    let cond_reg = self.read_into(&mut cur, *cond, SCRATCH_1)?;
                    self.insert_phi_copies(&mut cur, block.id, *else_block)?;
                    cur.term = PendingTerm::Jz {
                        cond_reg,
                        target: *else_block,
                    };
                    self.pending.push(cur);

                    let mut thunk = PendingBlock {
                        instructions: Vec::new(),
                        term: PendingTerm::Ret, // placeholder
                    };
                    self.insert_phi_copies(&mut thunk, block.id, *then_block)?;
                    thunk.term = PendingTerm::Jmp(*then_block);
                    self.pending.push(thunk);
                }
                Terminator::Return(value) => {
                    if let Some(v) = value {
                        let r = self.read_into(&mut cur, *v, SCRATCH_1)?;
                        if r != RET_REG {
                            cur.push(Instruction::new(Opcode::Mov, RET_REG, r, 0, 0));
                        }
                    }
                    cur.term = PendingTerm::Ret;
                    self.pending.push(cur);
                }
            }
        }

        Ok((self.pending, self.local_index))
    }
}

fn binop_opcode(op: BinOp) -> Opcode {
    match op {
        BinOp::Add => Opcode::Add,
        BinOp::Sub => Opcode::Sub,
        BinOp::Mul => Opcode::Mul,
        BinOp::Div => Opcode::Div,
        BinOp::Mod => Opcode::Mod,
        BinOp::CmpEq => Opcode::CmpEq,
        BinOp::CmpNe => Opcode::CmpNe,
        BinOp::CmpLt => Opcode::CmpLt,
        BinOp::CmpLe => Opcode::CmpLe,
        BinOp::CmpGt => Opcode::CmpGt,
        BinOp::CmpGe => Opcode::CmpGe,
        BinOp::And => Opcode::And,
        BinOp::Or => Opcode::Or,
    }
}

/// Lowers `func` to pending blocks (control-flow targets not yet
/// resolved to absolute addresses) plus the map from its own
/// `ir::BlockId`s to indices local to just this function's block list.
pub fn lower_function(
    func: &Function,
    alloc: &Allocation,
    region_base: u64,
    region_end: u64,
) -> Result<(Vec<PendingBlock>, HashMap<BlockId, usize>), CodegenError> {
    let next_temp_addr = region_base + (alloc.spills.len() as u64) * 8;
    Lowerer {
        func,
        alloc,
        liveness: liveness::analyze(func),
        pending: Vec::new(),
        local_index: HashMap::new(),
        next_temp_addr,
        region_end,
    }
    .lower()
}
