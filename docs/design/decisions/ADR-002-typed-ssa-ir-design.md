# ADR-002: Typed SSA IR — no dominance check, predecessors computed on demand, calls validated at module scope

## Status
Accepted

## Context

Ticket 002 needed to settle the IR's shape before ticket 003's parser
has anything to build against. Three design questions came up while
implementing it, each with a real correctness consequence:

1. Does a `BasicBlock` cache its own predecessor list, or is it derived?
2. Does `validate_function` check full SSA well-formedness (every use
   dominated by its definition), or only "defined somewhere in this
   function"?
3. Can a single function validate a `Call` instruction fully on its own?

## Decision

**Predecessors are computed on demand** (`Function::predecessors`),
never cached on `BasicBlock`. Ticket 003's parser builds a function's
blocks incrementally — a block's true predecessor set isn't known until
every block that might jump to it has been added, which happens at
unpredictable points during parsing (Braun et al.'s algorithm calls this
"sealing" a block). A cached `preds: Vec<BlockId>` field would need
active maintenance through that process and could silently go stale;
recomputing it from every block's terminator successors is cheap (this
IR's function sizes are never going to be large enough for it to matter)
and structurally can't drift.

**Validation checks "defined somewhere in the function," not
dominance.** True SSA well-formedness requires that every use is
dominated by its definition — computing that needs a dominator tree,
real extra machinery this ticket didn't need to justify yet. The weaker
check still catches the actual bug class ticket 002's own tests hit
during development (a genuine bug: function parameters weren't seeded
into the definition set at all, caught immediately by
`a_simple_straight_line_function_validates` failing) — undefined-value
typos and structural corruption. The gap (a use that references a real
definition, just not one that dominates it — e.g. a value defined only
on one branch of an `if`, used after the branches rejoin without a
`Phi`) is written down in `validate.rs`'s module doc comment rather than
silently assumed away, specifically because ticket 003's construction
algorithm produces IR where this can't happen *by construction*, so the
gap is currently unexercised, not currently unreachable-by-design.
Revisit if a later ticket ever constructs IR by a path other than
ticket 003's parser.

**`Call` signature checking happens at `validate_module`, not
`validate_function`.** A function can't know whether the function it
calls exists or what its signature is without seeing the rest of the
module — `validate_function` only checks that a `Call`'s arguments
reference defined values (a within-function fact); `validate_module`
runs every function's `validate_function` first, then makes a second
pass checking every `Call` site's callee, arity, argument types, and
result type against the actual target `Function`. This mirrors why
mentat's own `Program::validate` checks jump targets against
`self.blocks.len()` rather than trying to have `Block` validate itself.

## Consequences

- `validate_function(func)` alone is safe to call while iteratively
  building one function (as ticket 003 will do), without needing the
  whole module to exist yet. `validate_module` is the complete check a
  finished compilation unit must pass before codegen (ticket 004) or the
  reference interpreter run it.
- The interpreter (`ir::run`) documents that it assumes
  `validate_module` already accepted its input and does not re-check
  types or structure — only the one failure mode validation can't catch
  statically (division by zero) is a typed runtime error.
- Function parameters are pre-defined values at `ValueId(0)..
  ValueId(params.len())` by convention (not enforced by a type — a
  convention both `validate::collect_definitions` and
  `interp::call` independently rely on). Ticket 003's parser must honor
  this when assigning a function's first N value ids to its parameters;
  a future ADR should reconsider making this a checked invariant rather
  than a documented convention if it ever causes a real bug.
