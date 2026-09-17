# ADR-006: Generalized differential testing found a real interpreter bug; benchmarks measure the pipeline end to end

## Status
Accepted

## Context

Ticket 004's own differential test (straight-line arithmetic and `if`
only) already gave real confidence that codegen agreed with the
reference interpreter for the shapes it covered. Ticket 006's job was to
widen that net — specifically to bounded `while` loops, which stress
phi resolution across a back edge in a way straight-line code and a
single `if` cannot — and to add the benchmarks
`docs/design/LANGUAGE.md`'s Comparison section promised but ticket 004
didn't yet measure.

## Decision

**The generator now includes bounded `while` loops, guaranteed to
terminate by construction.** Rather than generating an arbitrary
condition and hoping it eventually becomes false, every loop this
generator (`crates/codegen/tests/gen/mod.rs`) emits has the fixed shape
`let __loopN = 0; while __loopN < K { <body>; __loopN = __loopN + 1; }`
for a small constant `K` — the counter and its increment are always
generated together as a unit, never left to arbitrary body statements,
so a generated program can never hang a property-test run regardless of
what ends up inside the loop body. Nesting depth is bounded the same
way `if` nesting already was.

**Recursion and multi-function calls stay out of the randomized
generator** — a deliberate scope boundary, not an oversight. Safely
generating an arbitrary call graph that's *guaranteed* to terminate
(rather than risk generating infinite or stack-overflowing mutual
recursion) is a harder problem than this ticket needs to solve.
Recursion correctness is instead covered by the hand-picked
representative programs already in `crates/codegen/tests/programs.rs`
and `crates/cli/tests/cli.rs` (`fact`, `is_even`/`is_odd`,
`sum_of_squares`) — real coverage, just not randomized.

## A real bug the wider generator found

Widening the generator to include nested `while` loops immediately
found a genuine, previously-latent bug — not in codegen, in **`ir`'s
reference interpreter** (ticket 002), which every earlier ticket had
been treating as ground truth. `ssa_builder` can legitimately construct
a `Phi` whose incoming operand, for some predecessor, is *another `Phi`
defined in the same block* — e.g. a value one nested loop's header
passes through unchanged, merged again at an outer loop's header. This
is correct, dominance-respecting SSA, but it demands a specific
evaluation discipline: every `Phi` in a block must resolve using the
environment exactly as it stood *before* entering the block, all of
them "simultaneously" — never a value another `Phi` in the same block
had *already updated this transition*.

`ir::interp`'s original main loop processed every instruction in a
block — `Phi`s included — strictly in textual order, writing each
result into the environment immediately. A later `Phi` referencing an
earlier same-block `Phi` would therefore see that `Phi`'s *freshly
computed* value for the current transition instead of its correct
pre-transition one, silently producing a wrong result for any program
whose loop nesting happened to trigger the pattern.

Concretely, for:

```
fn f() -> i64 {
    let c = 0; let i = 0;
    while i < 2 {
        a = c;       // reads c's value entering *this* iteration
        c = c + 1;
        i = i + 1;
    }
    return a;
}
```

the interpreter returned `0`; the mathematically correct answer (and
what codegen's compiled-and-VM-executed output already gave) is `1`.
**Codegen was already correct here for a real reason, not luck**: its
phi-elimination (`lower.rs`'s `parallel_move`, ADR-004) already stages
every predecessor's outgoing values through scratch memory before
writing any destination — exactly the "resolve all of them against
pre-transition values" discipline `ir::interp` was missing.

**Fix**: `ir::interp::call`'s block-execution loop now resolves every
`Phi` in a block in a first pass (against the environment as it stood
on entry, before any of *this* block's instructions run), commits all
of their results together, and only then executes the block's non-`Phi`
instructions. A dedicated regression test
(`ir::interp::tests::a_phi_referencing_another_phi_in_the_same_block_resolves_using_pre_transition_values`)
hand-builds this exact shape (bypassing the parser, since `ir` doesn't
depend on it) so the fix is pinned independently of whatever the
generator happens to produce on a given run.

## Benchmarks

### Compile time vs. program size (`benches/compile_time.rs`)

Straight-line programs of `n` sequential `v = v + 1;` reassignments,
parsing and codegen measured separately:

| n | parse | codegen |
|---|---|---|
| 10 | 8.4 µs | 41 µs |
| 100 | 60.4 µs | 2.43 ms |
| 500 | — | 66.1 ms |
| 1,000 | 594 µs | 259 ms |
| 10,000 | 6.91 ms | *(excluded — see below)* |

**Parsing scales close to linearly** (roughly 13–14x per 10x input,
i.e. slightly superlinear but not alarmingly so). **Codegen does not —
it's measurably quadratic** (~4x runtime for every 2x input size, exact
across 500→1,000). Root-caused, not just observed: every value in this
generator's programs lives in a *single basic block* (no branches or
loops), and `regalloc.rs`'s interference-graph construction is a naive
`for a in live { for b in live { ... } }` pairwise loop over
`liveness::live_within_block`'s result — an O(block size²) cost that
was already named as the *precision* trade-off of block-granularity
liveness (`liveness.rs`'s module doc, ADR-004) but whose *performance*
consequence hadn't been measured until this ticket. At 10,000
statements in one block this took several seconds *per codegen call*
during this benchmark's own development — informative to have measured
once (worth recording as a known, real limitation), not useful to make
every future `cargo bench` run pay for, so the benchmark's own size
range stops at 1,000.

**Consequence, honestly scoped rather than fixed here**: a future
ticket giving register allocation real per-value interval liveness
(instead of block-granularity) would turn this quadratic-in-block-size
cost back into something closer to linear — out of scope for ticket
006, which exists to *measure*, not to redesign the allocator ticket
004 already shipped and differentially tested.

### Compiled-and-VM-executed vs. interpreted runtime (`benches/runtime_vs_interpreter.rs`)

Both sides measured within the same `cargo bench` invocation (this
project's established methodology — see sietch's ADR-006: absolute
timings drift noticeably between separate invocations on real hardware,
so only same-run comparisons are trustworthy):

| program | interpreted | compiled + mentat VM | ratio |
|---|---|---|---|
| `fib(10)` | 64.9 µs | 2.62 ms | ~40x slower |
| `fib(15)` | 726 µs | 18.5 ms | ~25x slower |
| `fib(20)` | 8.12 ms | 211 ms | ~26x slower |
| `sum_to(100)` | 23.3 µs | 1.53 ms | ~66x slower |
| `sum_to(1,000)` | 231 µs | 4.94 ms | ~21x slower |
| `sum_to(10,000)` | 2.27 ms | 38.6 ms | ~17x slower |

**The honest, measured answer to `docs/design/LANGUAGE.md`'s Comparison
question is not the one that might be assumed going in**: compiling to
mentat bytecode and running it on mentat's VM is consistently *slower*
than this project's own from-scratch tree-walking reference
interpreter, by roughly one to two orders of magnitude, across both a
call/recursion-heavy program and a loop-heavy one. This is reported
directly rather than reframed, matching this project's established
practice of documenting a negative or surprising result plainly (see
sietch's ADR-004, a 12x regression reported honestly before it was
later fixed). Two concrete, plausible contributors, not yet
individually isolated (a natural follow-up measurement, not claimed
here as proven): mentat's dependency-driven scheduler
(`docs/design/decisions/` in the `mentat` repo) does real per-
instruction dependency-graph bookkeeping that a direct-dispatch tree
interpreter has no analog for, and this repo's own calling convention
(ADR-004) pushes register save/restore through real memory stores
around every `Call` — `fib`'s ratio being worse than `sum_to`'s (which
never calls anything) is consistent with call overhead being a real
factor, not the only one, since `sum_to` still shows a double-digit
slowdown with zero calls in it at all.

### Register-allocation spill rate vs. live-value count

Covered by `crates/codegen/src/regalloc.rs`'s existing unit test
(`more_concurrently_live_values_than_registers_forces_a_spill`, ticket
004) rather than a new criterion benchmark — spill *count* is a
discrete correctness/behavior property to assert on, not a timing to
measure, and criterion is specifically a timing tool; a scaling table
would need a bespoke harness this ticket judged not worth building for
the single data point this project's deliberately small v1 functions
actually exercise.

## Consequences

- Ticket 006 closes. Phase 3 (THE LANGUAGE)'s ticket backlog (001–006)
  is now fully done.
- `ir::interp` is now correct for the general case of same-block
  phi-referencing-phi, closing a real, previously-shipped-and-unnoticed
  bug in code every earlier ticket's tests had implicitly trusted as
  ground truth. This is exactly why differential testing against an
  independent implementation matters even after both sides pass their
  own hand-written tests — neither implementation's own test suite had
  a case shaped like this before ticket 006 widened the generator.
- The benchmark suite gives Phase 4+ (or a future performance-focused
  ticket in this repo) real baseline numbers to compare against, rather
  than starting from nothing — including two real, previously-unknown
  facts about this compiler's current state: register allocation is
  quadratic in single-block program size, and running compiled output
  on mentat's VM is currently *slower* than this project's own reference
  interpreter, by roughly one to two orders of magnitude, for every
  program shape measured. Neither is fixed by this ticket — ticket 006's
  job was to measure honestly, not to redesign either the allocator
  (ticket 004) or mentat's own scheduler (a different repo entirely).
  Both are legitimate targets for a future performance-focused ticket,
  tracked here rather than silently discovered and left unrecorded.
