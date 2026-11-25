# ADR-007: Short-circuit conditions in `while` loops

## Status
Accepted (bug fix found after Phase 3 closed)

## Context
ARRAKIS's public examples include a prime counter whose inner loop is
`while prime and d * d <= n { ... }`. It was rejected by validation with
`function 'is_prime': block b6 has no terminator`. Minimized:

```
fn main() -> i64 { let d = 0; while d < 3 and d < 5 { d = d + 1; } return d; }
```

Any `and`/`or` in a `while` condition failed; the same condition in an `if`
or a `let` compiled fine.

Cause: building SSA straight from the parser (ADR-003) means the short-circuit
operators create blocks *while the condition is being parsed*. `parse_and`
ends the current block (the loop header) with its own branch and leaves the
condition's value in a fresh join block. `parse_while` then terminated
`header`, not the block the condition ended in: it overwrote the
short-circuit branch and left the join block with no terminator. `parse_if`
did not have the bug because it already branched from `ctx.current_block`.

## Decision
`parse_while` records `cond_tail = ctx.current_block` after parsing the
condition and terminates that block with the loop branch. The back edge still
jumps to `header`, which re-evaluates the whole condition, so the
short-circuit blocks are inside the loop as they should be.

## Why the existing tests missed it
- The differential property generator (ADR-006) only emits loops of the form
  `while i < K`, never a boolean operator in a loop condition.
- The hand-written `while` tests all used single comparisons.

The IR validator caught the malformed function, so the bug produced a compile
error, never wrong code.

## Consequences
- New end-to-end test `short_circuit_conditions_in_while_loops` (`and`, `or`,
  a chained `a and not b or c`, and an early-exit search) runs on the real
  mentat VM. Mutation-checked: with the fix reverted the test fails.
- Lesson for this design: in a parser that emits SSA directly, every
  construct that consumes an expression must branch from the block the
  expression *ended* in, since an expression can now span several blocks.
  `let`, assignment, `return`, `if` and `while` were audited for this;
  only `while` was wrong.
