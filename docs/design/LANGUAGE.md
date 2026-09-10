# THE LANGUAGE — surface language and pipeline (Phase 3)

## Overview

CHAKOBSA compiles a small statically-typed, C-like language to
[mentat](https://github.com/n-3-0-l-d-3-v/mentat)'s (THE MACHINE) bytecode.
The whole point of this repo, per `docs/design/CONSTRAINTS.md`, is that the
front end never builds a conventional AST: parsing constructs a typed
SSA-form IR directly, and every later pass (type checking is folded into
construction; codegen lowers straight from IR to mentat's `Program`)
works on that IR, not on a tree.

## Pipeline

```text
source text
    |
  lexer (crates/lexer)      -- flat token stream, no separate token tree
    |
  parser (crates/parser)    -- builds typed SSA IR directly while parsing;
    |                          no intermediate AST is ever constructed
  ir (crates/ir)            -- typed SSA: functions, basic blocks, typed
    |                          values, phi nodes at merge points
  codegen (crates/codegen)  -- lowers IR to mentat::Program: virtual SSA
    |                          values get bounded physical registers
    |                          (linear-scan, spilling to memory once
    |                          NUM_REGISTERS=32 is exceeded)
  mentat::Program            -- run on THE MACHINE's VM
```

## Surface language (v1)

Deliberately small — real, not a toy, but scoped to what a from-scratch
typed-SSA-direct front end needs to prove itself, not a general-purpose
language. Extending the surface syntax later never requires touching the
"no AST" constraint; it's an orthogonal axis.

- Two types: `i64`, `bool`. No structs, arrays, or pointers in v1 (mentat
  itself only has a flat `u64`-addressed memory and i64-valued registers;
  a real type system with aggregates is future work, not required to
  prove out the "typed SSA directly from parsing" constraint).
- Top-level function declarations only: `fn name(param: type, ...) ->
  type { ... }`. No global variables, no closures, no modules — mentat's
  own model (fixed registers, block-structured programs) doesn't have a
  natural place for them, and this project's Definition of Done cares
  about real, working depth on the constraint at hand over breadth of
  surface features.
- Statements: `let` bindings (type inferred from the initializer),
  assignment to an existing binding, `if`/`else`, `while`, `return`,
  bare expression statements.
- Expressions: integer and boolean literals, variable references,
  function calls, unary `-`/`not`, binary arithmetic (`+ - * / %`),
  comparison (`== != < <= > >=`), and short-circuit `and`/`or`.
- Comments: `//` to end of line. No block comments, no strings (nothing
  in the surface language needs a string type yet).

## Why typed SSA, not AST-then-lower

A conventional pipeline parses to an AST, then runs a separate
AST-to-SSA lowering pass (usually via dominance-frontier computation).
This repo's constraint asks a sharper question: is the AST stage
actually necessary, or is it a convenient-but-skippable engineering
structure?

The answer used here is Braun et al.'s (2013) SSA-construction-without-
dominance-frontiers algorithm: it builds SSA form incrementally,
block-by-block, resolving each variable read against whatever
definitions currently reach it — inserting a phi (and recursively
resolving its operands) only when a block has multiple predecessors and
hasn't been "sealed" (all its predecessors are known) yet. Crucially,
this algorithm's natural driving loop — visit source constructs in
order, ask "what's the current value of variable X in the block I'm
building" — is exactly the shape of a recursive-descent parser's
control flow. That's what makes going straight from tokens to typed SSA
without an intermediate tree not just possible but the more natural
implementation, which is the concrete thing ticket 003 must demonstrate,
not just assert.

## Comparison

Ticket 006's differential testing compares the compiled-and-mentat-VM-
executed result of a program against a plain tree-walking reference
interpreter over the same typed SSA IR, for arbitrary well-typed
generated programs — the project's standard "compare against a
conventional reference" Definition-of-Done item, adapted to a compiler
(there's no pre-existing "conventional CHAKOBSA" to diff against, so the
reference is a from-scratch, deliberately naive interpreter over the
same IR that never goes anywhere near mentat, register allocation, or
codegen).
