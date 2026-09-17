---
status: done
phase: 3
---

# 005 — CLI toolchain (`chakobsac`)

A real command-line compiler driver, mirroring mentat's `imc` in spirit:
`chakobsac build|run|dump-ir <file>`. `run` compiles and executes the
result on mentat's VM in one step (this repo depends on mentat's `vm`
crate for that, not just `isa`), so a source program's result can be
observed end-to-end without a separate manual `imc run` step.

## Scope
- [x] `chakobsac build <file.ck> -o <file.ckp>`: compile to mentat
      bytecode, reusing mentat's own on-disk program format/
      serialization (plain `serde_json`-encoded `isa::Program`, no
      CHAKOBSA-specific wrapper) — verified directly interoperable with
      mentat's own `imc disasm`/`imc run` against a `chakobsac`-built
      artifact.
- [x] `chakobsac run <file.ck | file.ckp>`: compiles (if given source)
      then executes via mentat's `vm` crate, printing `main`'s return
      value. A required, zero-argument `fn main() -> i64` is the
      program's entry point — see ADR-005 for why.
- [x] `chakobsac dump-ir <file.ck>`: prints the typed SSA IR in a
      readable one-line-per-instruction text form
      (`crates/cli/src/dump_ir.rs`) — the debugging entry point every
      other ticket in this repo had previously been using ad hoc
      `{:#?}` Debug output for.
- [x] Integration tests: 7 tests against the actual built binary
      (`CARGO_BIN_EXE_chakobsac`) covering all three subcommands,
      recursion, a missing-`main` error, a parse error, and round-
      tripping `build` -> `run` on the resulting artifact — real
      example programs compiled and run end-to-end, not unit-level
      mocks.

See `docs/design/decisions/ADR-005-cli-toolchain.md` for the
required-`main` and plain-`isa::Program`-output design choices.
