# CHAKOBSA — THE LANGUAGE

> A compiler that avoids a conventional AST-centered pipeline.

## Why "CHAKOBSA"

The Fremen's actual battle/hunting language from the books — a real, named, constructed language within the universe, not just "a language exists here." Naming the compiler after a specific invented language (rather than a generic "tongue" or "code") mirrors the component's own constraint: build something recognizably language-like without the conventional structure (an AST) everyone assumes a language needs.

Part of **[ARRAKIS](https://github.com/n-3-0-l-d-3-v/arrakis)** — a constrained computing
ecosystem built by removing assumptions ordinary computers depend on. This
repository is developed standalone and mirrored into the combined ecosystem
repo commit-for-commit.

## Status

**Phase 3 — ACTIVE.** See
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

## Development

This is a real, tested, benchmarked systems component — not a demo. See
[docs/DEFINITION_OF_DONE.md](docs/DEFINITION_OF_DONE.md) for the acceptance
bar every piece of this repo must clear before it is considered complete.

```bash
cargo build
cargo test
cargo bench
```
