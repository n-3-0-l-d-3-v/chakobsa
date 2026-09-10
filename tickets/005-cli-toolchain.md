---
status: open
phase: 3
---

# 005 — CLI toolchain (`chakobsac`)

A real command-line compiler driver, mirroring mentat's `imc` in spirit:
`chakobsac build|run|dump-ir <file>`. `run` compiles and executes the
result on mentat's VM in one step (this repo depends on mentat's `vm`
crate for that, not just `isa`), so a source program's result can be
observed end-to-end without a separate manual `imc run` step.

## Scope
- `chakobsac build <file.ck> -o <file.imc>`: compile to mentat bytecode
  (reusing mentat's own on-disk program format/serialization).
- `chakobsac run <file.ck>`: compile then execute via mentat's `vm`
  crate, printing the program's result/exit trap.
- `chakobsac dump-ir <file.ck>`: print the typed SSA IR (ticket 002) in
  a readable text form — the debugging entry point for every other
  ticket in this repo, needed well before this ticket if a text IR
  printer gets pulled forward into ticket 002/003 instead (revisit
  ordering then).
- Integration tests: real `.ck` example programs (recursive functions,
  loops, nested conditionals) compiled and run end-to-end, checked
  against expected register/memory state after execution.

Not started. Depends on ticket 004.
