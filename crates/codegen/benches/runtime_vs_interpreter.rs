//! "Is going through mentat's bytecode actually faster than tree-
//! walking?" — `docs/design/LANGUAGE.md`'s Comparison section promised
//! this would be measured, not assumed. Both sides of each comparison
//! run within the same `cargo bench` invocation, per this project's
//! established methodology (see sietch's ADR-006): absolute numbers
//! drift noticeably between separate benchmark invocations on real
//! hardware (disk/CPU-cache warmth), so only same-run comparisons are
//! trustworthy.

use codegen::compile_module;
use codegen::regalloc::{RET_REG, STACK_PTR};
use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use ir::{run as interpret, Module, RtValue};
use isa::{Block, Instruction, Opcode, Program};
use vm::Vm;

fn compiled_program_calling(module: &Module, entry: &str, args: &[i64]) -> Program {
    let compiled = compile_module(module).unwrap();
    compiled.program.validate().unwrap();
    let entry_block = compiled.function_entry[entry];

    let mut blocks = compiled.program.blocks;
    let driver_block = blocks.len();
    let mut instructions: Vec<Instruction> = vec![Instruction::new(
        Opcode::LoadI,
        STACK_PTR,
        0,
        0,
        codegen::STACK_BASE as i32,
    )];
    instructions.extend(
        args.iter()
            .enumerate()
            .map(|(i, &v)| Instruction::new(Opcode::LoadI, i as u8, 0, 0, v as i32)),
    );
    instructions.push(Instruction::new(Opcode::Call, 0, 0, 0, entry_block as i32));
    blocks.push(Block {
        label: "bench_driver".to_string(),
        instructions,
    });
    blocks.push(Block {
        label: "bench_halt".to_string(),
        instructions: vec![Instruction::new(Opcode::Halt, 0, 0, 0, 0)],
    });

    let program = Program {
        blocks,
        entry: driver_block,
    };
    program.validate().unwrap();
    program
}

fn run_compiled(program: &Program) -> i64 {
    let mut vm = Vm::new(program.clone());
    vm.run().unwrap();
    vm.regs.get(RET_REG)
}

fn bench_recursive_fibonacci(c: &mut Criterion) {
    let src = r#"
        fn fib(n: i64) -> i64 {
            if n <= 1 { return n; } else { return fib(n - 1) + fib(n - 2); }
        }
    "#;
    let module = parser::parse(src).unwrap();
    ir::validate_module(&module).unwrap();

    let mut group = c.benchmark_group("fib_compiled_vs_interpreted");
    for n in [10i64, 15, 20] {
        let program = compiled_program_calling(&module, "fib", &[n]);
        group.bench_with_input(BenchmarkId::new("interpreted", n), &n, |b, &n| {
            b.iter(|| black_box(interpret(&module, "fib", &[RtValue::I64(n)]).unwrap()));
        });
        group.bench_with_input(
            BenchmarkId::new("compiled_vm", n),
            &program,
            |b, program| {
                b.iter(|| black_box(run_compiled(program)));
            },
        );
    }
    group.finish();
}

fn bench_loop_sum(c: &mut Criterion) {
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
    let module = parser::parse(src).unwrap();
    ir::validate_module(&module).unwrap();

    let mut group = c.benchmark_group("loop_sum_compiled_vs_interpreted");
    for n in [100i64, 1_000, 10_000] {
        let program = compiled_program_calling(&module, "sum_to", &[n]);
        group.bench_with_input(BenchmarkId::new("interpreted", n), &n, |b, &n| {
            b.iter(|| black_box(interpret(&module, "sum_to", &[RtValue::I64(n)]).unwrap()));
        });
        group.bench_with_input(
            BenchmarkId::new("compiled_vm", n),
            &program,
            |b, program| {
                b.iter(|| black_box(run_compiled(program)));
            },
        );
    }
    group.finish();
}

criterion_group!(benches, bench_recursive_fibonacci, bench_loop_sum);
criterion_main!(benches);
