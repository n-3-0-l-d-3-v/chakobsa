---
status: open
phase: 3
---

# 002 — Typed SSA IR representation

Define the IR data structures parsing (ticket 003) will build directly:
typed values, instructions, basic blocks with phi nodes, and functions —
plus structural validation (every value used is defined on every path
that reaches its use; every block ends in exactly one terminator; types
agree at every instruction), mirroring mentat's `Program::validate`
pattern of catching structural defects at construction/load time rather
than at codegen or runtime.

## Scope
- `Type`: `I64 | Bool`.
- `Value`: an opaque, typed SSA value id (`ValueId`), never reassigned
  once defined.
- `Instruction`: typed arithmetic/comparison/logical ops over `Value`s,
  a `Call`, a `Phi` (per-predecessor incoming values).
- `BasicBlock`: an ordered list of instructions plus exactly one
  terminator (`Jump`, `Branch` on a `Bool` value, or `Return`).
- `Function`: typed parameters, a return type, and a CFG of blocks.
- `Module`: a set of functions.
- Structural validation, analogous to `mentat::Program::validate`:
  reject a block with no terminator or a terminator that isn't last,
  reject a `Phi` whose incoming-value count doesn't match its block's
  predecessor count, reject a type mismatch between an instruction's
  declared result type and its operands' actual types.
- A reference tree-walking interpreter over this IR — not part of the
  compiler pipeline, but the independent implementation ticket 007's
  differential tests compare compiled output against.

Not started.
