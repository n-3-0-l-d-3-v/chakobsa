---
status: done
phase: 3
---

# 002 — Typed SSA IR representation

Define the IR data structures parsing (ticket 003) will build directly:
typed values, instructions, basic blocks with phi nodes, and functions —
plus structural validation (every value used is defined somewhere in the
function; every block ends in exactly one terminator; types agree at
every instruction), mirroring mentat's `Program::validate` pattern of
catching structural defects at construction/load time rather than at
codegen or runtime.

## Scope
- [x] `Type`: `I64 | Bool`.
- [x] `Value`: an opaque, typed SSA value id (`ValueId`), never
      reassigned once defined.
- [x] `Instruction`: typed arithmetic/comparison/logical ops
      (`BinOp`/`UnOp`) over `Value`s, `Call`, `Phi` (per-predecessor
      incoming values).
- [x] `BasicBlock`: an ordered list of instructions plus exactly one
      terminator (`Jump`, `Branch` on a `Bool` value, or `Return`).
- [x] `Function`: typed parameters, a return type, and a CFG of blocks.
      Predecessors are computed on demand (`Function::predecessors`),
      never cached — see ADR-002.
- [x] `Module`: a set of functions.
- [x] Structural validation, analogous to `mentat::Program::validate`:
      empty/missing-terminator blocks, unknown jump/branch targets, a
      `Phi` whose incoming set doesn't exactly match its block's actual
      predecessor set, a type mismatch between an instruction's declared
      result type and its operands' actual types, a mismatched `Call`
      arity/argument-type/result-type against the callee's real
      signature (checked at module scope — see ADR-002), a value used
      that was never defined. 22 unit tests plus 2 property tests (one
      confirming arbitrary generated arithmetic both validates and
      interprets to the independently-computed expected result, one
      confirming a corrupted operand reference is always caught as a
      typed `IrError`, never a validator panic).
- [x] A reference interpreter (`ir::run`) over this IR — a genuinely
      separate implementation from codegen (ticket 004), CFG-walking
      rather than tree-walking (there is no tree in this pipeline). This
      is what ticket 006's differential tests will compare compiled
      output against.

Found and fixed a real bug during development, not just theorized: the
first version of `validate_function` never seeded a function's
parameters into its definition set, so every straight-line function
using its own parameters failed validation — caught immediately by the
first hand-written test (`a_simple_straight_line_function_validates`),
fixed by seeding `ValueId(0)..ValueId(params.len())` from `func.params`
before scanning instructions.

See `docs/design/decisions/ADR-002-typed-ssa-ir-design.md` for the
on-demand-predecessors, no-dominance-check, and module-scope-call-
validation design choices.
