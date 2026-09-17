# CHAKOBSA — THE LANGUAGE

> A compiler that avoids a conventional AST-centered pipeline.

## Why "CHAKOBSA"

The Fremen's actual battle/hunting language from the books — a real, named, constructed language within the universe, not just "a language exists here." Naming the compiler after a specific invented language (rather than a generic "tongue" or "code") mirrors the component's own constraint: build something recognizably language-like without the conventional structure (an AST) everyone assumes a language needs.

Part of **[ARRAKIS](https://github.com/n-3-0-l-d-3-v/arrakis)** — a constrained computing
ecosystem built by removing assumptions ordinary computers depend on. This
repository is developed standalone and mirrored into the combined ecosystem
repo commit-for-commit.

## Status

**Phase 3 — COMPLETE.** All 6 tickets closed. See
[docs/design/LANGUAGE.md](docs/design/LANGUAGE.md) for the surface
language and full pipeline (lexer -> parser building typed SSA directly
-> codegen -> mentat bytecode).

**Ticket 001 (lexer) is done.** `crates/lexer`: a full token set for
the v1 surface language, greedy two-character-operator matching,
keyword-prefixed identifiers (`iffy`, `letter`) correctly lexing as
identifiers rather than a truncated keyword match, and malformed input
always a typed `LexError` — proven by a property test over completely
arbitrary byte input (not just hand-picked bad cases) plus a
render-then-relex round-trip property test. See
[ADR-001](docs/design/decisions/ADR-001-lexer-design.md) for the
fail-fast-vs-error-recovery and keyword-matching design choices.

**Ticket 002 (typed SSA IR) is done.** `crates/ir`: typed values,
instructions (`BinOp`/`UnOp`/`Call`/`Phi`), basic blocks, functions and
modules, plus structural + type validation mirroring mentat's
`Program::validate` — a `Phi`'s incoming set checked against its block's
*actual* predecessors (computed on demand, never cached, since ticket
003's parser builds blocks incrementally), a `Call` site checked against
its callee's real signature at module scope, and every value use
checked against a real definition. Also includes a from-scratch
reference interpreter (CFG-walking, not tree-walking — there's no tree
in this pipeline) that ticket 006's differential tests will compare
codegen'd-and-VM-executed results against. 22 unit tests plus 2 property
tests; one of the unit tests caught a real bug during development
(function parameters weren't seeded into the definition set) before it
could reach ticket 003. See
[ADR-002](docs/design/decisions/ADR-002-typed-ssa-ir-design.md) for the
on-demand-predecessors, no-dominance-check, and module-scope-call-
validation design choices.

**Ticket 003 (the parser) is done — this is the ticket that actually
proves the repo's whole premise.** `crates/parser` drives
`ssa_builder`'s implementation of Braun, Buchwald, Hack, Leißa, Mallon
& Zwinkau's (CC 2013) incremental SSA construction algorithm directly
from a recursive-descent/precedence-climbing expression and statement
parser — no `Expr`/`Stmt` AST node is ever built. Its driving loop
(resolve a variable read against whatever reaches this point; insert
and seal phis as a block's predecessors become known) turns out to
genuinely *be* the parser's own control flow, not something bolted on
top of it. Handles real programs end-to-end: straight-line arithmetic,
`if`/`else` with and without an `else`, `while` loops (including
nested), recursion, mutual recursion (via a two-pass signature scan
that resolves forward references before any body is parsed), and
short-circuit `and`/`or` compiled as real branches — proven to actually
skip evaluating their right-hand side, not just to produce the right
boolean. 13 representative end-to-end program tests, 17 typed-error
tests, and 2 property tests (arbitrary byte input never panics the
parser; arbitrary generated well-typed programs always produce IR that
passes full `ir::validate_module`). Caught a real bug along the way — an
`if`/`else` where both arms `return` could produce an empty,
unreachable join block that failed validation — fixed and documented
honestly rather than papered over. See
[ADR-003](docs/design/decisions/ADR-003-parser-direct-to-ssa.md).

**Ticket 004 (codegen to mentat) is done — the first real cross-repo
integration in the ecosystem.** `crates/codegen` depends on mentat's
`isa`/`vm` crates via a `git` dependency and lowers typed SSA to
`mentat::Program`: greedy register allocation bounded by mentat's 32
registers (spilling to memory once exceeded), a genuine software call
stack so recursive calls can't clobber a caller's own still-needed
values (mentat's own call stack tracks only return addresses, never
registers), and parallel (not sequential) multi-value moves for call
arguments and phi elimination so an argument swap like `sub(b, a)`
can't clobber itself mid-move. Bridges three real architectural
mismatches between a typed-SSA CFG and mentat's block-indexed,
register-scarce, dependency-scheduled machine — see
[ADR-004](docs/design/decisions/ADR-004-codegen-and-calling-convention.md)
for all of them, plus four real bugs this ticket's own tests caught
against the **real mentat VM** (not just the reference interpreter):
mentat's bitwise `Not` opcode being mistaken for logical negation, the
caller-saved-register hazard recursion exposed, and two independent
liveness-analysis bugs (phi operands attributed to the wrong block;
upward-exposed-use miscounted) — each fixed and covered by a regression
test. 13 end-to-end tests run compiled output on the real VM, 9 unit
tests cover liveness/regalloc directly, and a 200-case differential
property test confirms compiled-and-VM-executed results always match
the reference interpreter.

**Ticket 005 (`chakobsac` CLI) is done.** `build`/`run`/`dump-ir`, a
real command-line compiler driver mirroring mentat's own `imc`. A
required, zero-argument `fn main() -> i64` is the program's entry point
(see [ADR-005](docs/design/decisions/ADR-005-cli-toolchain.md) for why);
`build`'s output is a plain, undecorated `isa::Program` — no
CHAKOBSA-specific wrapper — so it's directly consumable by mentat's own
`imc disasm`/`imc run` with zero glue code, verified directly rather
than just claimed. `dump-ir` gets its own readable one-line-per-
instruction renderer, replacing the ad hoc `{:#?}` dumps every earlier
ticket had been using. 7 integration tests run against the actual built
binary, covering all three subcommands, recursion, a missing-`main`
error, a parse error, and round-tripping `build` -> `run`.

**Ticket 006 (differential testing and benchmarks) is done — this
closes Phase 3 (THE LANGUAGE) in full.** The property-test generator now
includes bounded `while` loops (terminating by construction — the loop
counter and its increment are always generated as one unit), which
immediately found a real, previously-shipped bug in `ir`'s reference
interpreter (ticket 002), not in codegen: a `Phi` can legitimately
reference *another `Phi` in the same block* (a value a nested loop
passes through unchanged), and the interpreter resolved every
instruction — `Phi`s included — strictly in textual order, so a later
`Phi` could observe an earlier one's just-updated value instead of its
correct pre-transition one. Codegen was already right here (its phi
elimination already stages everything through scratch memory first).
Fixed, with a dedicated regression test pinning the exact shape.
Benchmarks measure what `docs/design/LANGUAGE.md` promised: compile
time scales close to linearly for parsing but is **measurably
quadratic** for codegen on single-block programs (root-caused to
register allocation's O(block-size²) interference-graph construction —
a real, honestly-reported limitation, not fixed here), and running
compiled output on mentat's VM is **currently slower than this
project's own reference interpreter**, by roughly one to two orders of
magnitude across both a recursion-heavy and a loop-heavy program — a
genuinely surprising, plainly reported result rather than the outcome
one might have assumed going in. See
[ADR-006](docs/design/decisions/ADR-006-differential-testing-and-benchmarks.md)
for the full measured tables and root-causing.

See [tickets/](tickets/) for the live phase-by-phase ticket board and
[docs/design/](docs/design/) for constraints, invariants and architecture
decision records.

## The constraint

The compiler avoids a conventional multi-pass AST-centered architecture, preferring a typed SSA/CPS-like representation directly from parsing.

## What the constraint forces

Alternative intermediate representation design, and a compiler front-end that must justify every conventional structure it keeps.

## Research question

> Which compiler abstractions are fundamental, and which are merely convenient engineering structures?

## Sibling repositories

- [mentat](https://github.com/n-3-0-l-d-3-v/mentat) — THE MACHINE (COMPLETE)
- [muaddib](https://github.com/n-3-0-l-d-3-v/muaddib) — THE KERNEL (QUEUED)
- [sietch](https://github.com/n-3-0-l-d-3-v/sietch) — THE VAULT (COMPLETE)
- [choam](https://github.com/n-3-0-l-d-3-v/choam) — THE DATABASE (QUEUED)
- [distrans](https://github.com/n-3-0-l-d-3-v/distrans) — THE WIRE (QUEUED)
- [landsraad](https://github.com/n-3-0-l-d-3-v/landsraad) — THE COLONY (QUEUED)
- [ghola](https://github.com/n-3-0-l-d-3-v/ghola) — THE HISTORY (QUEUED)
- [shai-hulud](https://github.com/n-3-0-l-d-3-v/shai-hulud) — THE ARTIFACT (STRETCH)

## Usage

```bash
cargo run -p chakobsac -- run examples/fact.ck        # compile + execute, prints main's return value
cargo run -p chakobsac -- dump-ir examples/fact.ck     # readable typed SSA IR
cargo run -p chakobsac -- build examples/fact.ck -o fact.ckp  # -> a plain isa::Program, runnable by mentat's own imc too
```

## Development

This is a real, tested, benchmarked systems component — not a demo. See
[docs/DEFINITION_OF_DONE.md](docs/DEFINITION_OF_DONE.md) for the acceptance
bar every piece of this repo must clear before it is considered complete.

```bash
cargo build
cargo test
cargo bench
```
