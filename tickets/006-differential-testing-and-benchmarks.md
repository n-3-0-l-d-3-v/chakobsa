---
status: open
phase: 3
---

# 006 — Differential testing and benchmarks

The Definition-of-Done items this repo hasn't earned yet even once
tickets 001–005 close: randomized end-to-end testing and measured
performance, not just hand-written example programs passing.

## Scope
- A generator for arbitrary well-typed small programs (bounded
  expression depth, bounded loop iteration counts to keep execution
  finite) usable as a `proptest` strategy.
- Differential test: for each generated program, the result from
  compiling through `chakobsac` and running on mentat's VM must match
  the result from the reference tree-walking interpreter over the same
  IR (ticket 002) — the "compare against a conventional reference"
  Definition-of-Done item, with the interpreter playing that role since
  there's no pre-existing conventional CHAKOBSA to diff against.
- Benchmarks: compile time vs. program size; register-allocation
  spill rate vs. live-value count at representative program shapes;
  compiled-and-VM-executed runtime vs. the reference interpreter's
  runtime for the same programs, to give a real, measured "is going
  through mentat's bytecode actually faster than tree-walking"
  comparison rather than an assumed one.

Not started. Depends on ticket 005.
