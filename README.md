# impossible-language — THE LANGUAGE

> A compiler that avoids a conventional AST-centered pipeline.

Part of **[The Impossible Computer](https://github.com/n-3-0-l-d-3-v/impossible-computer)** — a constrained computing
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

- [impossible-machine](https://github.com/n-3-0-l-d-3-v/impossible-machine) — THE MACHINE (ACTIVE)
- [impossible-kernel](https://github.com/n-3-0-l-d-3-v/impossible-kernel) — THE KERNEL (QUEUED)
- [impossible-vault](https://github.com/n-3-0-l-d-3-v/impossible-vault) — THE VAULT (QUEUED)
- [impossible-database](https://github.com/n-3-0-l-d-3-v/impossible-database) — THE DATABASE (QUEUED)
- [impossible-wire](https://github.com/n-3-0-l-d-3-v/impossible-wire) — THE WIRE (QUEUED)
- [impossible-colony](https://github.com/n-3-0-l-d-3-v/impossible-colony) — THE COLONY (QUEUED)
- [impossible-history](https://github.com/n-3-0-l-d-3-v/impossible-history) — THE HISTORY (QUEUED)
- [impossible-artifact](https://github.com/n-3-0-l-d-3-v/impossible-artifact) — THE ARTIFACT (STRETCH)

## Development

This is a real, tested, benchmarked systems component — not a demo. See
[docs/DEFINITION_OF_DONE.md](docs/DEFINITION_OF_DONE.md) for the acceptance
bar every piece of this repo must clear before it is considered complete.

```bash
cargo build
cargo test
cargo bench
```
