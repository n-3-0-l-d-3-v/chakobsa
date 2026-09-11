# ADR-003: The parser builds typed SSA directly — Braun et al., two-pass signatures, and the both-branches-terminate edge case

## Status
Accepted

## Context

This is the ticket that has to make `docs/design/LANGUAGE.md`'s central
claim real, not just asserted: can a parser build typed SSA form
directly, with no intermediate AST, as *more natural* implementation
than the conventional parse-to-AST-then-lower pipeline — not merely
possible in principle? Three concrete design questions had to be
answered to find out, and one implementation bug surfaced during testing
that's worth recording because of what it reveals about the approach's
actual sharp edges.

## Decision

**Braun, Buchwald, Hack, Leißa, Mallon & Zwinkau's (CC 2013) incremental
SSA construction algorithm**, driving directly off the parser's own
control flow (`crates/parser/src/ssa_builder.rs`). A variable read
resolves against whatever definition currently reaches the block being
built; a block that isn't "sealed" yet (not every predecessor known —
true for a loop header until its body is fully parsed) gets an
*incomplete* phi, resolved once `seal_block` runs. This is exactly the
paper's algorithm, and its driving loop genuinely is the parser's own
recursive-descent traversal — `parse_if`/`parse_while`/`parse_and`/
`parse_or` call `new_block`/`terminate`/`seal_block`/`read_variable`
inline as they recognize each construct. No `Expr`/`Stmt` node is ever
built; `crate::parser::Parser` holds only a token cursor, a lexical
scope stack (name -> `Type`, for compile-time visibility — a different
concern from the SSA builder's per-block "current value" tracking), and
the function signature table.

**Signatures are scanned in a first pass, bodies parsed in a second.**
A single-pass parser can't resolve a call to a function declared later
in the same file, and can't support mutual recursion at all (each of two
mutually-recursive functions needs the other's signature before either
body can be type-checked). Pass 1 parses every function's header (name,
parameter types, return type) and skips its body via brace matching;
pass 2 re-parses each function from scratch with the complete signature
table already built. Re-parsing the header a second time is cheap and
avoids threading parsed-header state between passes.

**Short-circuit `and`/`or` are real control flow, not eager `Bin`
instructions.** `ir::BinOp::And`/`Or` exist (ticket 002) but this parser
deliberately never emits them — `a and b` lowers to a branch on `a`,
skipping `b`'s evaluation entirely when `a` is false, merged back with a
plain phi (two known incoming values, no incomplete-phi bookkeeping
needed since both branches' predecessors are final the moment they're
created). This is the one place ticket 003 generates control flow from
an *expression* rather than a statement, and it's what makes
`10 / x > 1` inside `x != 0 and 10 / x > 1` provably never execute when
`x` is `0` (tested directly — see `crates/parser/tests/programs.rs`, a
version that didn't short-circuit would trip `DivideByZero` instead of
returning `false`).

## A real bug this ticket's own tests caught

The first version of `parse_if` always created a join block after an
`if`/`else`, unconditionally jumping both arms' tails into it. When
*both* arms end in their own terminator (e.g. both `return`), that join
block ends up with **zero predecessors, zero instructions, and no
terminator** — nothing was ever going to fill it in, and
`ir::validate`'s `EmptyBlock` check correctly rejected it. Caught
immediately by `if_else_branch_and_phi` and `recursive_function`
(`fact`'s `if n <= 1 { return 1 } else { return n * fact(n-1) }` hits
this exact shape) failing validation, not by inspection. Fixed by
special-casing "both arms already terminated": no join block is created
at all, and `current_block` is left pointing at one of the already-
terminated tails, so the enclosing block-parsing loop's unreachable-code
check does the right thing for whatever (if anything) follows.

This is left as a **known limitation, not fully engineered around**:
v1 does no reachability/dead-code analysis. If both arms of an `if`
terminate and the function's grammar requires more statements to follow
syntactically (it doesn't, in general), those statements would correctly
be flagged `UnreachableCode` by the same mechanism — but there's no
attempt to prove a *partial* fallthrough is dead in more complex cases
(e.g. one arm of a nested `if` returning). This is judged acceptable for
v1's scope; a future ticket adding proper reachability analysis would
subsume this special case rather than needing to preserve it.

## Alternatives Considered
1. Full two-pass parse-to-AST-then-lower — the conventional approach
   this whole repo exists to avoid; rejected per the project's own
   constraint, not on independent technical merit.
2. Single-pass parsing with forward-reference calls left unresolved
   until a later linking step — rejected: it would reintroduce a
   post-parse pass this ticket's whole point is to avoid needing, for
   the sake of avoiding re-parsing function headers (a genuinely cheap
   operation).

## Consequences

- Ticket 003 closes. `crates/lexer` -> `crates/parser` (driving
  `crates/ir` directly) is now a complete, tested, working pipeline from
  source text to validated typed SSA, for real programs: straight-line
  arithmetic, if/else with and without else, while loops (including
  nested), recursion, mutual recursion (across the two-pass signature
  scan), and short-circuit `and`/`or`.
- `ir::BinOp::And`/`Or` remain defined in `ir` but unused by this
  parser — not dead code in the ecosystem sense (ticket 002's own tests
  exercise them, and a future non-short-circuiting language extension
  or a different front end could use them), but worth noting so it
  isn't mistaken for an oversight.
- Codegen (ticket 004) inherits a real IR shape to lower: every
  function's blocks, including every `and`/`or`'s synthetic merge
  blocks, must map onto mentat blocks, and every phi (loop-carried
  variables included) needs out-of-SSA copy insertion at lowering time.
