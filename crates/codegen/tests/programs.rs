//! End-to-end codegen tests: real `.ck` source, parsed (ticket 003),
//! validated, compiled to `isa::Program` (this ticket), and actually run
//! on mentat's real `vm::Vm` — not the reference interpreter. Every
//! expected value is hand-verified independently, the same bar
//! `crates/parser/tests/programs.rs` set for the parser.
//!
//! Since mentat's `Call`/`Ret` need a call stack entry to return into,
//! and a bare `Ret` with an empty call stack traps
//! (`Trap::CallStackUnderflow`), every test runs the target function
//! through a small driver: two extra blocks appended after the compiled
//! program that load the arguments into the calling convention's
//! argument registers, `Call` the target, then `Halt` so the result
//! (left in `RET_REG` by the callee) can be read out of the VM's final
//! register file.

use codegen::compile_module;
use codegen::regalloc::{RET_REG, STACK_PTR};
use isa::{Block, Instruction, Opcode, Program};
use parser::parse;
use vm::{ExitReason, Vm};

/// Compiles `src`, then runs `entry(args)` on the real mentat VM and
/// returns the value left in `RET_REG`.
fn exec(src: &str, entry: &str, args: &[i64]) -> i64 {
    let module = parse(src).unwrap_or_else(|e| panic!("parse error: {e}"));
    ir::validate_module(&module).unwrap_or_else(|e| panic!("ir failed validation: {e}"));
    let compiled = compile_module(&module).unwrap_or_else(|e| panic!("codegen error: {e}"));
    compiled
        .program
        .validate()
        .unwrap_or_else(|e| panic!("generated mentat program failed validate(): {e}"));

    let entry_block = compiled.function_entry[entry];
    let mut blocks = compiled.program.blocks;
    let driver_block = blocks.len();

    // The driver's own entry bypasses the compiled program's embedded
    // stack-pointer init block (that block's Jmp target is whichever
    // function happens to be first in the module, not necessarily the
    // one under test here), so it must initialize STACK_PTR itself
    // before any call-crossing register save/restore can run correctly.
    let mut driver_instructions = vec![Instruction::new(
        Opcode::LoadI,
        STACK_PTR,
        0,
        0,
        codegen::STACK_BASE as i32,
    )];
    driver_instructions.extend(
        args.iter()
            .enumerate()
            .map(|(i, &v)| Instruction::new(Opcode::LoadI, i as u8, 0, 0, v as i32)),
    );
    driver_instructions.push(Instruction::new(Opcode::Call, 0, 0, 0, entry_block as i32));
    blocks.push(Block {
        label: "driver_call".to_string(),
        instructions: driver_instructions,
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
        .unwrap_or_else(|e| panic!("driver-wrapped program failed validate(): {e}"));

    let mut vm = Vm::new(program);
    let exit = vm
        .run()
        .unwrap_or_else(|trap| panic!("VM trapped: {trap:?}"));
    assert_eq!(exit, ExitReason::Halted);
    vm.regs.get(RET_REG)
}

#[test]
fn straight_line_arithmetic() {
    let src = "fn add(a: i64, b: i64) -> i64 { return a + b; }";
    assert_eq!(exec(src, "add", &[3, 4]), 7);
}

#[test]
fn operator_precedence() {
    let src = "fn f() -> i64 { return 2 + 3 * 4 - 1; }";
    assert_eq!(exec(src, "f", &[]), 13);
}

#[test]
fn if_else_branch() {
    let src = r#"
        fn abs(x: i64) -> i64 {
            if x < 0 { return -x; } else { return x; }
        }
    "#;
    assert_eq!(exec(src, "abs", &[-7]), 7);
    assert_eq!(exec(src, "abs", &[7]), 7);
}

#[test]
fn if_without_else_uses_a_real_phi() {
    let src = r#"
        fn clamp_positive(x: i64) -> i64 {
            let y = x;
            if x < 0 { y = 0; }
            return y;
        }
    "#;
    assert_eq!(exec(src, "clamp_positive", &[-5]), 0);
    assert_eq!(exec(src, "clamp_positive", &[5]), 5);
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
    assert_eq!(exec(src, "sum_to", &[5]), 10);
    assert_eq!(exec(src, "sum_to", &[0]), 0);
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
    assert_eq!(exec(src, "count_pairs", &[3]), 9);
}

#[test]
fn recursive_function() {
    let src = r#"
        fn fact(n: i64) -> i64 {
            if n <= 1 { return 1; } else { return n * fact(n - 1); }
        }
    "#;
    assert_eq!(exec(src, "fact", &[5]), 120);
}

#[test]
fn mutually_recursive_functions() {
    let src = r#"
        fn is_even(n: i64) -> bool {
            if n == 0 { return true; } else { return is_odd(n - 1); }
        }
        fn is_odd(n: i64) -> bool {
            if n == 0 { return false; } else { return is_even(n - 1); }
        }
    "#;
    assert_eq!(exec(src, "is_even", &[10]), 1);
    assert_eq!(exec(src, "is_odd", &[10]), 0);
}

#[test]
fn a_call_used_inside_a_larger_expression() {
    let src = r#"
        fn square(x: i64) -> i64 { return x * x; }
        fn sum_of_squares(a: i64, b: i64) -> i64 { return square(a) + square(b); }
    "#;
    assert_eq!(exec(src, "sum_of_squares", &[3, 4]), 25);
}

#[test]
fn a_call_with_swapped_arguments_does_not_clobber_them() {
    // Exercises parallel_move directly: `a` and `b` are swapped's own
    // parameters, pre-colored to registers 0 and 1 respectively. Calling
    // sub(b, a) needs b's value in register 0 and a's value in register
    // 1 — the exact opposite of where they already live. A naive
    // sequential Mov(reg0 <- b) followed by Mov(reg1 <- a) would clobber
    // register 0 (overwriting `a`) before the second move ever reads it,
    // silently passing the wrong value.
    let src = r#"
        fn sub(x: i64, y: i64) -> i64 { return x - y; }
        fn swapped(a: i64, b: i64) -> i64 { return sub(b, a); }
    "#;
    assert_eq!(exec(src, "swapped", &[10, 3]), -7); // sub(3, 10) = -7
}

#[test]
fn short_circuit_and_never_evaluates_the_rhs_when_the_lhs_is_false() {
    let src = r#"
        fn f(x: i64) -> bool {
            return x != 0 and 10 / x > 1;
        }
    "#;
    assert_eq!(exec(src, "f", &[0]), 0); // short-circuits; 10/0 would trap otherwise
    assert_eq!(exec(src, "f", &[1]), 1);
    assert_eq!(exec(src, "f", &[20]), 0);
}

#[test]
fn short_circuit_or_never_evaluates_the_rhs_when_the_lhs_is_true() {
    let src = r#"
        fn f(x: i64) -> bool {
            return x == 0 or 10 / x > 1;
        }
    "#;
    assert_eq!(exec(src, "f", &[0]), 1);
    assert_eq!(exec(src, "f", &[20]), 0);
    assert_eq!(exec(src, "f", &[1]), 1);
}

#[test]
fn not_and_boolean_literals() {
    let src = "fn f(x: bool) -> bool { return not x; }";
    assert_eq!(exec(src, "f", &[1]), 0);
    assert_eq!(exec(src, "f", &[0]), 1);
}
