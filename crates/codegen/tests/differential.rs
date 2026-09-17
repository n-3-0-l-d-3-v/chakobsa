//! Differential property test: for arbitrary generated well-typed
//! programs — now including bounded `while` loops, not just straight-
//! line and `if` statements — compiling to mentat and running on the
//! real VM must produce exactly the same result as `ir::run`'s
//! independent reference interpreter. The two implementations share
//! nothing below the validated IR they both start from (this is ticket
//! 002's reference interpreter and ticket 004's codegen); this is
//! ticket 006's generalization of that comparison.
//!
//! **Scope boundary, not an oversight**: this generator produces a
//! single function with `let`/assignment/`if`/bounded-`while`
//! statements — no user-defined function calls or recursion. Safely
//! generating an arbitrary call graph that's *guaranteed* to terminate
//! (rather than risk generating infinite or stack-overflowing mutual
//! recursion) is a harder problem than this ticket needs to solve;
//! recursion and multi-function call correctness are instead covered by
//! the hand-picked representative programs in
//! `crates/codegen/tests/programs.rs` and `crates/cli/tests/cli.rs`
//! (`fact`, `is_even`/`is_odd`, `sum_of_squares`) — real coverage, just
//! not randomized. See
//! `docs/design/decisions/ADR-006-differential-testing-and-benchmarks.md`.

use codegen::compile_module;
use codegen::regalloc::{RET_REG, STACK_PTR};
use ir::{run as interpret, RtValue};
use isa::{Block, Instruction, Opcode, Program};
use parser::parse;
use proptest::prelude::*;

mod gen;
use gen::arb_program;

/// Compiles `module`, runs `f()` on the real mentat VM via the same
/// driver-block pattern the other codegen tests use, and returns the
/// value left in `RET_REG`.
fn run_on_vm(module: &ir::Module) -> i64 {
    let compiled = compile_module(module).expect("codegen must accept a validated module");
    compiled
        .program
        .validate()
        .expect("codegen must always emit a valid mentat program");

    let entry_block = compiled.function_entry["f"];
    let mut blocks = compiled.program.blocks;
    let driver_block = blocks.len();
    blocks.push(Block {
        label: "driver_call".to_string(),
        instructions: vec![
            Instruction::new(Opcode::LoadI, STACK_PTR, 0, 0, codegen::STACK_BASE as i32),
            Instruction::new(Opcode::Call, 0, 0, 0, entry_block as i32),
        ],
    });
    blocks.push(Block {
        label: "driver_halt".to_string(),
        instructions: vec![Instruction::new(Opcode::Halt, 0, 0, 0, 0)],
    });

    let program = Program {
        blocks,
        entry: driver_block,
    };
    program
        .validate()
        .expect("driver-wrapped program must stay valid");

    let mut vm = vm::Vm::new(program);
    vm.run()
        .expect("VM must not trap on a well-typed generated program");
    vm.regs.get(RET_REG)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(300))]
    #[test]
    fn compiled_and_interpreted_results_always_agree(src in arb_program()) {
        let module = parse(&src).unwrap_or_else(|e| panic!("generated program failed to parse: {e}\n{src}"));
        ir::validate_module(&module).unwrap_or_else(|e| panic!("generated program's IR failed validation: {e}\n{src}"));

        let interpreted = interpret(&module, "f", &[])
            .unwrap_or_else(|e| panic!("reference interpreter failed: {e}\n{src}"));
        let compiled_result = run_on_vm(&module);

        prop_assert_eq!(
            interpreted,
            Some(RtValue::I64(compiled_result)),
            "compiled (mentat VM) and interpreted results disagree for:\n{}",
            src
        );
    }
}
