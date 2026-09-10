# Scope — chakobsa

## CORE (required for this repo to be considered complete at all)
- Lexer (ticket 001).
- Typed SSA IR representation + structural validation + reference
  interpreter (ticket 002).
- Parser that builds the typed SSA IR directly, no intermediate AST
  (ticket 003) — the ticket that actually proves this repo's constraint.
- Code generation to `mentat::Program`, including register allocation
  bounded by mentat's `NUM_REGISTERS` and a documented calling
  convention (ticket 004).

## EXTENSION (required for full integration into the combined ecosystem)
- `chakobsac` CLI: build/run/dump-ir, running compiled programs on
  mentat's VM end-to-end (ticket 005).
- Differential testing against the reference interpreter and
  benchmarks (ticket 006).

## EXPERIMENT (only attempted once CORE + EXTENSION are healthy)
- Structs/aggregates and a real type system beyond `i64`/`bool`.
- Closures or first-class functions (mentat has no natural home for
  them — would need its own constraint-compatible design first).
- A second codegen backend, to see how much of the front end (lexer,
  IR, parser) is genuinely target-independent versus quietly
  mentat-shaped.
