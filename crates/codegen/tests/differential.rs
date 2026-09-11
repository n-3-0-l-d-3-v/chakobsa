//! Differential property test: for arbitrary generated well-typed
//! programs, compiling to mentat and running on the real VM must
//! produce exactly the same result as `ir::run`'s independent reference
//! interpreter — the two implementations share nothing below the
//! validated IR they both start from (this is ticket 002's reference
//! interpreter and this ticket's codegen, an early instance of what
//! ticket 006 will generalize into the project's full differential
//! testing story).

use codegen::compile_module;
use codegen::regalloc::{RET_REG, STACK_PTR};
use ir::{run as interpret, RtValue};
use isa::{Block, Instruction, Opcode, Program};
use parser::parse;
use proptest::prelude::*;
use vm::Vm;

/// Same small generated-program grammar `crates/parser/tests/property.rs`
/// uses — arbitrary `i64`-only straight-line/branching programs over a
/// fixed small variable set, so every reference is guaranteed to
/// resolve and every generated program is well-typed by construction.
fn arb_program() -> impl Strategy<Value = String> {
    static VAR_NAMES: [&str; 3] = ["a", "b", "c"];

    let arb_atom = prop_oneof![
        (0i64..100).prop_map(|n| n.to_string()),
        prop::sample::select(&VAR_NAMES[..]).prop_map(|s| s.to_string()),
    ];

    let arb_expr = arb_atom.prop_recursive(3, 20, 4, |inner| {
        prop_oneof![(inner.clone(), "[+\\-*]", inner.clone())
            .prop_map(|(a, op, b)| format!("({a} {op} {b})")),]
    });

    let arb_cond = (arb_expr.clone(), "[<>]|==|!=", arb_expr.clone())
        .prop_map(|(a, op, b)| format!("{a} {op} {b}"));

    let arb_stmt = prop_oneof![
        (prop::sample::select(&VAR_NAMES[..]), arb_expr.clone())
            .prop_map(|(v, e)| format!("{v} = {e};")),
        arb_cond.clone().prop_flat_map(move |cond| {
            let v = VAR_NAMES[0];
            (Just(cond), 0i64..50).prop_map(move |(cond, n)| format!("if {cond} {{ {v} = {n}; }}"))
        }),
    ];

    prop::collection::vec(arb_stmt, 1..8).prop_map(move |stmts| {
        format!(
            "fn f() -> i64 {{ let a = 0; let b = 0; let c = 0; {} return a; }}",
            stmts.join(" ")
        )
    })
}

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

    let mut vm = Vm::new(program);
    vm.run()
        .expect("VM must not trap on a well-typed generated program");
    vm.regs.get(RET_REG)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]
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
