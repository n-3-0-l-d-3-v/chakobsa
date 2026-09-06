# CHAKOBSA — THE LANGUAGE

> A compiler that avoids a conventional AST-centered pipeline.

## Why "CHAKOBSA"

The Fremen's actual battle/hunting language from the books — a real, named, constructed language within the universe, not just "a language exists here." Naming the compiler after a specific invented language (rather than a generic "tongue" or "code") mirrors the component's own constraint: build something recognizably language-like without the conventional structure (an AST) everyone assumes a language needs.

Part of **[ARRAKIS](https://github.com/n-3-0-l-d-3-v/arrakis)** — a constrained computing
ecosystem built by removing assumptions ordinary computers depend on. This
repository is developed standalone and mirrored into the combined ecosystem
repo commit-for-commit.

## Status

**Phase 3 — QUEUED**

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
- [sietch](https://github.com/n-3-0-l-d-3-v/sietch) — THE VAULT (ACTIVE)
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
