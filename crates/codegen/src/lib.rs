//! Lowers CHAKOBSA's typed SSA IR (`ir`) to mentat's `isa::Program` — the
//! first real cross-repo integration point in the ecosystem: this crate
//! depends on mentat's `isa` crate directly (a `git` dependency on the
//! `mentat` repository, resolved by Cargo like any other dependency).
//!
//! See `docs/design/decisions/ADR-004-codegen-and-calling-convention.md`
//! for the calling convention, register reservations, and the real
//! architectural mismatches this lowering has to bridge (mentat's `Jz`/
//! `Jnz` name only one branch target; `Call` is a terminator but
//! `ir::Call` is not; multi-value moves must be parallel, not
//! sequential).

pub mod liveness;
pub mod lower;
pub mod regalloc;

use std::collections::HashMap;

use ir::Module;
use isa::{Block, Instruction, Opcode, Program};

use lower::PendingTerm;
use regalloc::{RegAllocError, STACK_PTR};

/// Each function gets this many bytes of memory for its spill slots and
/// call-argument staging temporaries — generous for this language's
/// deliberately small v1 surface area (see `regalloc`'s module doc),
/// and checked (`CodegenError::OutOfSpillSpace`) rather than silently
/// overrun if a function ever needs more.
const FUNCTION_REGION_BYTES: u64 = 4096;

/// Base address of the software call stack `lower` pushes caller-live
/// registers through around every `Call` (see `lower`'s module doc and
/// ADR-004). Placed well clear of every function's static spill/temp
/// region (`FUNCTION_REGION_BYTES` each) so the two can never collide —
/// checked, not just assumed, in `compile_module`.
pub const STACK_BASE: u64 = 0x80000; // 512 KiB into mentat's default 1 MiB memory

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum CodegenError {
    #[error(transparent)]
    RegAlloc(#[from] RegAllocError),
    #[error("function '{func}' needs more spill/temporary space than its {FUNCTION_REGION_BYTES}-byte budget")]
    OutOfSpillSpace { func: String },
    #[error("function '{func}' has an i64 constant ({value}) that doesn't fit in mentat's 32-bit immediate field — arbitrary 64-bit constant materialization isn't implemented")]
    ConstantTooLarge { func: String, value: i64 },
    #[error("call to '{callee}', which no function in this module defines (should have been caught by ir::validate_module already)")]
    UnknownCallee { callee: String },
    #[error("{count} functions * {FUNCTION_REGION_BYTES}-byte regions would overlap the call stack at {STACK_BASE:#x}")]
    TooManyFunctionsForMemoryLayout { count: usize },
}

pub struct CompiledModule {
    pub program: Program,
    /// Every function's entry block, as an absolute index into
    /// `program.blocks` — how a caller (a test harness, or ticket 005's
    /// `chakobsac run`) picks which function to actually execute, since
    /// `program.entry` alone can only point at one of them.
    pub function_entry: HashMap<String, usize>,
}

/// Compiles every function in `module` into one flat `isa::Program`
/// (mentat has no notion of separate compilation units — `Call`'s target
/// is an absolute block index, so every function's blocks live in the
/// same array). `program.entry` defaults to the module's first
/// function; use `function_entry` to run a different one instead (see
/// `crates/codegen/tests/programs.rs`'s driver-block pattern).
pub fn compile_module(module: &Module) -> Result<CompiledModule, CodegenError> {
    if (module.functions.len() as u64) * FUNCTION_REGION_BYTES > STACK_BASE {
        return Err(CodegenError::TooManyFunctionsForMemoryLayout {
            count: module.functions.len(),
        });
    }
    let reserved_arg_registers = module
        .functions
        .iter()
        .map(|f| f.params.len())
        .max()
        .unwrap_or(0) as u8;

    // Pass 1: lower every function independently. Intra-function control
    // flow (Jmp/Jz) is left as ir::BlockId; Call targets are left as
    // callee names. Neither can be resolved to an absolute mentat block
    // index yet — that depends on every earlier function's final block
    // count, which we only know once all of them are lowered.
    let mut owned_pending: Vec<(String, lower::PendingBlock)> = Vec::new();
    let mut function_entry: HashMap<String, usize> = HashMap::new();
    let mut local_indices: HashMap<String, HashMap<ir::BlockId, usize>> = HashMap::new();

    for (i, func) in module.functions.iter().enumerate() {
        let region_base = (i as u64) * FUNCTION_REGION_BYTES;
        let region_end = region_base + FUNCTION_REGION_BYTES;
        let alloc = regalloc::allocate(func, region_base, reserved_arg_registers)?;
        let (pending, local_index) = lower::lower_function(func, &alloc, region_base, region_end)?;

        function_entry.insert(func.name.clone(), owned_pending.len());
        for block in pending {
            owned_pending.push((func.name.clone(), block));
        }
        local_indices.insert(func.name.clone(), local_index);
    }

    // Pass 2: every function's absolute base and internal block-index
    // map are now known, so resolve every pending terminator to a real
    // mentat instruction with a concrete absolute target. Every function
    // block index is shifted by 1 to make room for the stack-pointer
    // init block prepended at absolute index 0 below.
    let mut blocks = Vec::with_capacity(owned_pending.len() + 1);
    for (i, (owner, pending)) in owned_pending.iter().enumerate() {
        let local_index = &local_indices[owner];
        let resolve = |b: ir::BlockId| 1 + function_entry[owner] + local_index[&b];

        let terminator = match &pending.term {
            PendingTerm::Jmp(target) => {
                Instruction::new(Opcode::Jmp, 0, 0, 0, resolve(*target) as i32)
            }
            PendingTerm::Jz { cond_reg, target } => {
                Instruction::new(Opcode::Jz, 0, *cond_reg, 0, resolve(*target) as i32)
            }
            PendingTerm::Call { callee } => {
                let target =
                    1 + *function_entry
                        .get(callee)
                        .ok_or_else(|| CodegenError::UnknownCallee {
                            callee: callee.clone(),
                        })?;
                Instruction::new(Opcode::Call, 0, 0, 0, target as i32)
            }
            PendingTerm::Ret => Instruction::new(Opcode::Ret, 0, 0, 0, 0),
        };

        let mut instructions = pending.instructions.clone();
        instructions.push(terminator);
        blocks.push(Block {
            label: format!("{owner}::{i}"),
            instructions,
        });
    }
    for entry in function_entry.values_mut() {
        *entry += 1;
    }

    // Stack-pointer init: mentat's registers start zero-initialized, so
    // STACK_PTR needs one explicit LoadI before any call-crossing save
    // ever runs. This block always becomes absolute index 0.
    let module_entry = module
        .functions
        .first()
        .map(|f| function_entry[&f.name])
        .unwrap_or(1);
    let init_block = Block {
        label: "init_stack_ptr".to_string(),
        instructions: vec![
            Instruction::new(Opcode::LoadI, STACK_PTR, 0, 0, STACK_BASE as i32),
            Instruction::new(Opcode::Jmp, 0, 0, 0, module_entry as i32),
        ],
    };
    blocks.insert(0, init_block);

    Ok(CompiledModule {
        program: Program { blocks, entry: 0 },
        function_entry,
    })
}
