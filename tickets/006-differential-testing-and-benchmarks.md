---
status: done
phase: 3
---

# 006 — Differential testing and benchmarks

The Definition-of-Done items this repo hadn't earned yet even once
tickets 001–005 close: randomized end-to-end testing at a wider scope
than ticket 004's own straight-line/`if`-only generator, and measured
performance, not just hand-written example programs passing.

## Scope
- [x] A generator for arbitrary well-typed small programs, now including
      bounded `while` loops guaranteed to terminate by construction
      (the loop counter and its increment are always generated together
      as a unit, never left to arbitrary body statements) —
      `crates/codegen/tests/gen/mod.rs`, usable as a `proptest` strategy.
- [x] Differential test: for each generated program (300 cases per run),
      the result from compiling through codegen and running on mentat's
      VM must match the result from the reference interpreter over the
      same IR — `crates/codegen/tests/differential.rs`. **Found a real,
      previously-shipped bug**: `ir::interp` resolved a block's `Phi`
      instructions strictly in textual order rather than treating them
      as simultaneous against pre-transition values, giving a wrong
      result whenever one `Phi` legitimately referenced another `Phi` in
      the same block (a nested-loop pass-through value) — codegen was
      already correct here (its `parallel_move` phi elimination already
      had the right discipline). Fixed in `ir::interp`, with a dedicated
      regression test pinning the exact shape independent of the
      generator. See
      `docs/design/decisions/ADR-006-differential-testing-and-benchmarks.md`.
- [x] Benchmarks:
      - Compile time vs. program size (`benches/compile_time.rs`):
        parsing and codegen measured separately over 10–10,000-statement
        programs.
      - Compiled-and-VM-executed runtime vs. reference-interpreter
        runtime (`benches/runtime_vs_interpreter.rs`): recursive `fib`
        and a `while`-loop `sum_to`, both measured in the same benchmark
        run — the concrete answer to
        `docs/design/LANGUAGE.md`'s Comparison section, measured rather
        than assumed.
      - Register-allocation spill rate vs. live-value count: covered by
        `regalloc.rs`'s existing
        `more_concurrently_live_values_than_registers_forces_a_spill`
        unit test (ticket 004) rather than a new criterion benchmark —
        spill count is a discrete behavior to assert, not a timing to
        measure; see ADR-006 for why a scaling-table benchmark wasn't
        judged worth building for this project's deliberately small v1
        function sizes.

Recursion and multi-function calls are deliberately excluded from the
randomized generator — generating an arbitrary call graph *guaranteed*
to terminate is a harder problem than this ticket needs to solve.
Recursion correctness is covered by the hand-picked representative
programs already in `crates/codegen/tests/programs.rs` and
`crates/cli/tests/cli.rs`.

**This closes Phase 3 (THE LANGUAGE)'s ticket backlog (001–006) in
full.**
