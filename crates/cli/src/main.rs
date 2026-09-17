//! `chakobsac` — CHAKOBSA's command-line toolchain, mirroring mentat's
//! `imc` in spirit: build/run/dump-ir, all built on the shared
//! `parser`/`ir`/`codegen` crates rather than duplicating logic.

mod dump_ir;

use std::fs;
use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use codegen::regalloc::STACK_PTR;
use isa::{Block, Instruction, Opcode, Program};
use vm::Vm;

#[derive(Parser)]
#[command(name = "chakobsac", about = "The Language toolchain", version)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Compiles a `.ck` source file to a runnable mentat program
    /// (`.ckp`, JSON — loadable by mentat's own `imc run`/`imc disasm`
    /// too, since it's a plain `isa::Program`). The source must define a
    /// zero-argument `fn main() -> i64`, which becomes the program's
    /// entry point.
    Build {
        input: PathBuf,
        #[arg(short, long)]
        output: PathBuf,
    },
    /// Compiles and immediately runs a `.ck` source file's `main`, or
    /// runs an already-`build`-compiled `.ckp` program directly. Prints
    /// `main`'s return value.
    Run { input: PathBuf },
    /// Prints the typed SSA IR (post-parse, pre-codegen) in a readable
    /// text form — the debugging entry point for every other ticket in
    /// this repo.
    DumpIr { input: PathBuf },
}

fn main() -> Result<()> {
    match Cli::parse().command {
        Command::Build { input, output } => cmd_build(&input, &output),
        Command::Run { input } => cmd_run(&input),
        Command::DumpIr { input } => cmd_dump_ir(&input),
    }
}

fn parse_and_validate(input: &PathBuf) -> Result<ir::Module> {
    let source =
        fs::read_to_string(input).with_context(|| format!("reading {}", input.display()))?;
    let module = parser::parse(&source).map_err(|e| anyhow::anyhow!("{e}"))?;
    ir::validate_module(&module)
        .map_err(|e| anyhow::anyhow!("internal error: compiler produced invalid IR: {e}"))?;
    Ok(module)
}

/// Compiles `module` and wraps it into a self-contained, directly-
/// runnable program: `main` must take no arguments, and the wrapper
/// initializes `STACK_PTR` (see
/// `docs/design/decisions/ADR-004-codegen-and-calling-convention.md`)
/// before calling it, then halts so the result sits in the VM's final
/// register file — the same driver-block pattern
/// `crates/codegen/tests/programs.rs` uses, promoted here to the
/// actual product.
fn compile_runnable(module: &ir::Module) -> Result<Program> {
    if module.function("main").map(|f| f.params.len()) != Some(0) {
        bail!("source must define a zero-argument `fn main() -> i64`");
    }
    let compiled =
        codegen::compile_module(module).map_err(|e| anyhow::anyhow!("codegen error: {e}"))?;
    compiled.program.validate().map_err(|e| {
        anyhow::anyhow!("internal error: codegen produced an invalid mentat program: {e}")
    })?;

    let main_entry = compiled.function_entry["main"];
    let mut blocks = compiled.program.blocks;
    let driver_block = blocks.len();
    blocks.push(Block {
        label: "chakobsac_driver".to_string(),
        instructions: vec![
            Instruction::new(Opcode::LoadI, STACK_PTR, 0, 0, codegen::STACK_BASE as i32),
            Instruction::new(Opcode::Call, 0, 0, 0, main_entry as i32),
        ],
    });
    blocks.push(Block {
        label: "chakobsac_halt".to_string(),
        instructions: vec![Instruction::new(Opcode::Halt, 0, 0, 0, 0)],
    });

    let program = Program {
        blocks,
        entry: driver_block,
    };
    program.validate().map_err(|e| {
        anyhow::anyhow!("internal error: the driver-wrapped program is invalid: {e}")
    })?;
    Ok(program)
}

fn cmd_build(input: &PathBuf, output: &PathBuf) -> Result<()> {
    let module = parse_and_validate(input)?;
    let program = compile_runnable(&module)?;
    fs::write(output, serde_json::to_string_pretty(&program)?)
        .with_context(|| format!("writing {}", output.display()))?;
    println!(
        "compiled {} block(s) -> {}",
        program.blocks.len(),
        output.display()
    );
    Ok(())
}

fn load_or_compile(input: &PathBuf) -> Result<Program> {
    if input.extension().and_then(|e| e.to_str()) == Some("ck") {
        let module = parse_and_validate(input)?;
        compile_runnable(&module)
    } else {
        let text =
            fs::read_to_string(input).with_context(|| format!("reading {}", input.display()))?;
        serde_json::from_str(&text)
            .with_context(|| format!("parsing compiled program {}", input.display()))
    }
}

fn cmd_run(input: &PathBuf) -> Result<()> {
    let program = load_or_compile(input)?;
    program.validate().map_err(|e| anyhow::anyhow!("{e}"))?;
    let mut vm = Vm::new(program);
    match vm.run() {
        Ok(exit) => {
            for line in &vm.output {
                println!("{line}");
            }
            println!("{}", vm.regs.get(codegen::regalloc::RET_REG));
            eprintln!("exit: {exit:?}");
            Ok(())
        }
        Err(trap) => bail!("trapped: {trap}"),
    }
}

fn cmd_dump_ir(input: &PathBuf) -> Result<()> {
    let module = parse_and_validate(input)?;
    print!("{}", dump_ir::render(&module));
    Ok(())
}
