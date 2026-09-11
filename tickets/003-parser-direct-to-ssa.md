---
status: done
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
- [x] Expression parsing (precedence climbing) producing `Value`s
      directly: no `Expr` AST node is ever constructed.
      `crates/parser/src/parser.rs`'s `parse_or`/`parse_and`/`parse_not`/
      `parse_cmp`/`parse_add`/`parse_mul`/`parse_unary`/`parse_primary`
      each emit `ir` instructions and return `(ValueId, Type)` the
      instant an operator/operand is recognized.
- [x] Statement parsing (`let`, assignment, `if`/`else`, `while`,
      `return`, expression statements) threading the current block and
      the live variable-to-value environment as parsing proceeds.
- [x] Type checking folded into construction: a binary op's operand
      types are checked the moment both operands are already `Value`s
      with known `Type`s; a type error is a parse-time `ParseError`, not
      a later pass.
- [x] Parse errors (unexpected token, undefined variable/function, type
      mismatch, wrong argument count/types at a call site, unreachable
      code after a terminating statement) are typed and carry a `Span`,
      never a panic — proven by a property test over arbitrary byte
      input, not just hand-picked cases.
- [x] Property/differential test: `arbitrary_generated_programs_always_produce_valid_ir`
      generates small well-typed programs and requires every one to
      pass `ir::validate_module` — the parser's job is to make ill-formed
      IR structurally impossible to emit, checked directly rather than
      assumed. 13 hand-written representative-program tests (straight-
      line arithmetic, if/else with and without an else branch, while
      loops including nested ones, recursion, mutual recursion via the
      two-pass signature scan, and short-circuit `and`/`or` proven to
      actually skip evaluating their right-hand side) plus 17 error-path
      unit tests.

**Braun et al.'s incremental, dominance-frontier-free SSA construction
algorithm does turn out to be the more natural implementation, not just
a possible one** — its driving loop (resolve a variable read against
whatever reaches this point; insert/seal phis as predecessor sets become
known) is exactly a recursive-descent parser's own control flow, with no
separate tree ever needed in between. See
`docs/design/decisions/ADR-003-parser-direct-to-ssa.md`, which also
documents a real bug the ticket's own tests caught (an `if`/`else` where
both arms terminate could produce an empty, unreachable, terminator-less
join block) and a deliberate, documented v1 limitation (no dead-code/
reachability analysis beyond that one case).
