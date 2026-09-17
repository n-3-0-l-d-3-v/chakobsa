//! A readable text rendering of the typed SSA IR — `chakobsac dump-ir`'s
//! whole job, and the debugging entry point every other ticket in this
//! repo has been using ad hoc (via `{:#?}`) until now.

use std::fmt::Write;

use ir::{InstKind, Module, Terminator};

pub fn render(module: &Module) -> String {
    let mut out = String::new();
    for (i, func) in module.functions.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        render_function(&mut out, func);
    }
    out
}

fn render_function(out: &mut String, func: &ir::Function) {
    let params: Vec<String> = func
        .params
        .iter()
        .map(|(n, t)| format!("{n}: {t}"))
        .collect();
    let ret = func.ret_ty.map(|t| format!(" -> {t}")).unwrap_or_default();
    let _ = writeln!(out, "fn {}({}){} {{", func.name, params.join(", "), ret);
    for block in &func.blocks {
        let entry_marker = if block.id == func.entry {
            " (entry)"
        } else {
            ""
        };
        let _ = writeln!(out, "  {}{entry_marker}:", block.id);
        for inst in &block.instructions {
            let _ = writeln!(out, "    {} = {}", inst.id, render_kind(&inst.kind));
        }
        if let Some(term) = &block.terminator {
            let _ = writeln!(out, "    {}", render_terminator(term));
        }
    }
    let _ = writeln!(out, "}}");
}

fn render_kind(kind: &InstKind) -> String {
    match kind {
        InstKind::ConstI64(n) => format!("const {n}"),
        InstKind::ConstBool(b) => format!("const {b}"),
        InstKind::Bin(op, a, b) => format!("{} {a}, {b}", op.mnemonic()),
        InstKind::Un(op, a) => format!("{} {a}", op.mnemonic()),
        InstKind::Call { func, args } => {
            let args: Vec<String> = args.iter().map(|a| a.to_string()).collect();
            format!("call {func}({})", args.join(", "))
        }
        InstKind::Phi(incoming) => {
            let parts: Vec<String> = incoming
                .iter()
                .map(|(b, v)| format!("[{b} -> {v}]"))
                .collect();
            format!("phi {}", parts.join(", "))
        }
    }
}

fn render_terminator(term: &Terminator) -> String {
    match term {
        Terminator::Jump(b) => format!("jmp {b}"),
        Terminator::Branch {
            cond,
            then_block,
            else_block,
        } => {
            format!("br {cond}, {then_block}, {else_block}")
        }
        Terminator::Return(Some(v)) => format!("ret {v}"),
        Terminator::Return(None) => "ret".to_string(),
    }
}
