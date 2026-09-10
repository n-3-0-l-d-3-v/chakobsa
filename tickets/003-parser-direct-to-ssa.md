---
status: open
phase: 3
---

# 003 — Parser: builds typed SSA directly, no intermediate AST

The centerpiece ticket for this repo's whole constraint. A recursive-
descent / Pratt expression parser that, while parsing, incrementally
constructs the IR from ticket 002 using Braun et al.'s
dominance-frontier-free SSA construction algorithm (see
`docs/design/LANGUAGE.md`) — variable reads resolve against whatever
definition currently reaches them in the block being built, with phi
insertion (and "incomplete phi" resolution once a block is sealed)
happening inline as control flow constructs are parsed, never as a
separate pass over a pre-built tree.

## Scope
- Expression parsing (precedence climbing) producing `Value`s directly:
  no `Expr` AST node is ever constructed.
- Statement parsing (`let`, assignment, `if`/`else`, `while`, `return`,
  expression statements) threading the current block and the live
  variable-to-value environment as parsing proceeds.
- Type checking folded into construction: a binary op's operand types
  are checked the moment both operands are already `Value`s with known
  `Type`s; a type error is a parse-time `ParseError`, not a later pass.
- Parse errors (unexpected token, undefined variable, type mismatch,
  wrong argument count/types at a call site) are typed and carry a
  `Span`, never a panic.
- Property/differential test: for a corpus of generated well-typed
  programs, the IR built directly by the parser must be behaviorally
  identical (per the reference interpreter from ticket 002) to a
  hand-verified expected result for representative programs, and must
  never fail SSA/type validation on its own output.

Not started. This is the ticket that has to actually demonstrate the
"parse straight to typed SSA is more natural than AST-then-lower, not
just possible" claim in `docs/design/LANGUAGE.md` — if the resulting
parser ends up building an implicit tree in disguise (e.g. a deep
`Expr` enum under a different name), that's a signal the claim needs
revisiting honestly in an ADR, not papering over.
