# ADR-005: `chakobsac` — a zero-argument `main`, and build output that's a plain, mentat-interoperable `isa::Program`

## Status
Accepted

## Context

Ticket 005 needed to settle what "the program's entry point" even means.
Every function in this language declares its own parameters and return
type (`docs/design/LANGUAGE.md`); nothing marks one as special. mentat's
own `imc run` just executes whatever `Program.entry` names — a single,
argument-free starting point, since the machine itself has no notion of
"the OS passed me argv." A compiler driver has to bridge that gap
somehow before "run this source file" is a meaningful command.

## Decision

**`main` is a required, zero-argument, `i64`-returning function** —
`fn main() -> i64 { ... }` — mirroring the convention nearly every
compiled language uses for exactly the same reason: an unambiguous,
argument-free place for a whole program's execution to start.
`chakobsac build`/`run` reject a source file missing it with a plain
`anyhow` error, not a panic or a confusing downstream mentat trap.

**`build`'s output is a plain, undecorated `isa::Program`** — the same
JSON shape mentat's own `imc asm` produces — rather than a
CHAKOBSA-specific wrapper format. `compile_runnable` appends the driver-
block pattern `crates/codegen/tests/programs.rs` already established
(initialize `STACK_PTR`, `Call` `main`, `Halt`) directly onto the
compiled program and points `Program.entry` at it, so the artifact
`chakobsac build` writes is immediately usable by mentat's *own* `imc
run`/`imc disasm` with no CHAKOBSA-specific tooling at all — verified
directly: `imc disasm` against a `chakobsac`-built `fact.ck` prints
correct mentat disassembly, register save/restore and all. This is a
second real cross-repo integration payoff from ticket 004's work, not
just a nice-to-have: two independently-developed tools in two separate
repositories interoperate through nothing but the shared, versioned
`isa::Program` shape.

**`dump-ir` gets its own small text renderer** (`crates/cli/src/
dump_ir.rs`) rather than reusing `{:#?}` Debug output. Every other
ticket in this repo had been dumping IR via ad hoc `Debug` prints during
development; a real one-line-per-instruction rendering
(`v3 = add v1, v2`, `br v2, b1, b2`) is both the actual ticket
deliverable and now the standing debugging tool for every ticket after
this one.

## Alternatives Considered
1. No required `main`; `run` takes a `--entry <name>` flag naming which
   function to execute (with CLI-supplied arguments for its parameters)
   — rejected as unnecessary generality for what a v1 compiler needs
   to be usable; every test program in this repo already reads
   naturally with a `main`, and threading CLI-parsed arguments into
   arbitrary parameter types (only `i64`/`bool` exist, so this wouldn't
   even be hard) wasn't worth the added surface area yet.
2. A CHAKOBSA-specific build-output wrapper (e.g. embedding
   `function_entry` metadata alongside the program) — rejected: it would
   have broken interoperability with mentat's own tooling for no benefit
   this ticket's scope actually needs (nothing currently needs to `run`
   a *non-`main`* function from a built artifact).

## Consequences

- Ticket 005 closes. `chakobsac build|run|dump-ir` is a complete, real
  command-line compiler driver: 7 integration tests exercise all three
  subcommands against the actual built binary (`CARGO_BIN_EXE_chakobsac`),
  including a clean, panic-free error path for a missing `main`, a parse
  error, and round-tripping `build` -> `run` on the resulting artifact.
- `chakobsac`-built programs are directly consumable by mentat's `imc`
  toolchain (disassembly, `imc run`, `imc replay`) with zero glue code —
  demonstrated, not just claimed, against `imc disasm`.
- A future ticket that needs to run something other than a zero-arg
  `main` (e.g. a test harness wanting to call an arbitrary function with
  arbitrary arguments) still has `codegen::compile_module`'s lower-level
  `function_entry` map available directly — `chakobsac`'s `main`-only
  convention is a CLI policy choice, not a codegen limitation.
