# ADR-004: Codegen to mentat — calling convention, register reservations, and three real architectural mismatches

## Status
Accepted

## Context

Ticket 004 is this project's first real cross-repo integration: `chakobsa`
now depends on `mentat`'s `isa` (and, for testing, `vm`) crates via a
`git` dependency, resolved by Cargo exactly like any other dependency.
Lowering typed SSA (ticket 002/003) to mentat's `Program` surfaced three
genuine architectural mismatches between "a CFG of typed SSA values" and
"mentat's block-indexed, register-scarce, dependency-scheduled machine"
that a naive one-instruction-at-a-time translator would get wrong — and,
in this ticket's own development, initially did.

## Decision

### Register layout

32 physical registers, reserved as:

| Register | Purpose |
|---|---|
| 0..reserved_arg_registers | Argument passing — see "the caller-saved hazard" below |
| reserved_arg_registers..27 | General allocation pool |
| 27 (`STACK_PTR`) | Software call stack for caller-live-value save/restore |
| 28, 29 (`SCRATCH_1/2`) | Reloading spilled operands / staging a spilled result |
| 30 (`ZERO_REG`) | Always 0 (mentat registers start zero-initialized; never written) |
| 31 (`RET_REG`) | Return value, written by callee before `Ret`, read by caller after `Call` |

Register allocation is greedy graph coloring over a **block-granularity**
interference graph (two values conflict if co-live anywhere in the same
block) — a deliberate precision trade-off documented in `liveness.rs`:
always conservative (can't miss a real interference), sometimes
imprecise (more register pressure than an interval allocator), judged
acceptable given mentat's generous register count and this language's
deliberately small v1 surface area. Parameters are pre-colored
(`ValueId(i)` -> register `i`) since a callee simply reads whatever is
in its parameter registers at entry — mentat's `Call` has no argument-
passing mechanism of its own.

### Three real mismatches, and how lowering bridges them

1. **mentat's `Jz`/`Jnz` name only one branch target**; the other is
   whichever block is physically next in `Program::blocks`. Rather than
   controlling global block layout to exploit that (fragile, and it
   would tangle codegen's block ordering with every other concern),
   every `ir::Branch` lowers to **two** physical mentat blocks: one
   ending in `Jz` to the else-target, immediately followed by a one-
   instruction "thunk" block that unconditionally `Jmp`s to the then-
   target. Correct regardless of how anything else is laid out — see
   `lower.rs`'s module doc.

2. **mentat's `Call` is a terminator**; control resumes at whatever is
   physically next once `Ret` fires. But `ir::Call` is an ordinary,
   value-producing instruction that can sit in the middle of a block
   (`let r = f(x); return r + 1;`). Every block containing a `Call`
   therefore splits into two mentat blocks at that point — the second
   starting with fetching the result out of `RET_REG`.

3. **Multi-value moves must be parallel, not sequential.** Both call
   arguments and phi elimination move several values at once, and a
   naive `Mov`-per-pair sequence can clobber a value before it's read
   if two moves alias registers (a swap: `sub(b, a)` when `a`/`b` are
   the caller's own parameters in registers 0/1). `parallel_move` stages
   every source into scratch memory before writing any destination —
   correct independent of aliasing. **Caught by this ticket's own tests**:
   `a_call_with_swapped_arguments_does_not_clobber_them` fails
   immediately under a naive sequential-`Mov` implementation.

### The caller-saved-register hazard (found during testing, fixed properly)

Two further real bugs surfaced only once actual programs were run on
mentat's real VM (not just validated statically):

- **mentat's `Not` is bitwise** (`!1 == -2` in two's-complement), not
  logical negation. Since this language's booleans are always `0`/`1`,
  logical `not x` lowers to `CmpEq(x, 0)` instead — caught by
  `not_and_boolean_literals` expecting `0`, getting `-2`.
- **Calling any function — especially recursively — can clobber a
  caller's own live values with no warning**, because mentat's `Call`/
  `Ret` only track a return-*address* stack (block indices), never
  register contents. `fact(n)`'s own `n`, still needed after
  `fact(n-1)` returns to compute `n * result`, was being silently
  overwritten by the recursive call's own use of the same physical
  register. Caught by `recursive_function` (`fact(5)` returning `1`
  instead of `120`) and `a_call_used_inside_a_larger_expression`.

  Fixed with a genuine software call stack: at every `Call` site, every
  register-allocated value block-granularity-live in the same block
  (except the call's own not-yet-defined result) is `Store`d to
  `[STACK_PTR + i*8]`, `STACK_PTR` is bumped by that count, the call
  runs, `STACK_PTR` is popped back, and every saved value is `Load`ed
  back. Because `STACK_PTR` is an ordinary register that survives across
  `Call`/`Ret` like any other, nested and recursive invocations
  naturally get non-overlapping save regions — real, dynamic,
  per-invocation storage, not a per-function static slot (which would
  have hit exactly the same corruption one level down). `STACK_PTR` is
  initialized once, in a synthetic block `compile_module` prepends at
  absolute index 0, since mentat's registers start at `0` and this is
  the one register that must not.

## Known limitations, documented rather than engineered around

- **Spill slots remain per-function static addresses**, not stack-
  relative. A recursive function whose live-across-call values are
  register-allocated is now safe (via the mechanism above); one that
  additionally needs to *spill* a value that's simultaneously live
  across a recursive call would have each active invocation corrupt the
  others' spill slots. No test program in this ticket spills (32
  registers comfortably covers every test function's live-value count),
  so this is real but currently unexercised — the natural follow-up
  would make spill slots relative to `STACK_PTR` too, unifying both
  mechanisms.
- **Arbitrary 64-bit constants aren't materialized.** mentat's `imm`
  field is `i32`; an `i64` constant outside that range is a typed
  `CodegenError::ConstantTooLarge`, not silently truncated or wrong.
- **`parallel_move` stages every value through memory**, even when a
  direct register-to-register `Mov` would do — simple and always
  correct, not fast. Performance tuning is out of scope until ticket
  006's benchmarks show it matters.

## Measured / verified result

- 13 end-to-end tests actually running compiled output on mentat's real
  `vm::Vm` (not just the reference interpreter) — straight-line
  arithmetic, branches, loops (including nested), recursion, mutual
  recursion, argument-swapping, and short-circuit `and`/`or` proven to
  actually skip evaluating their right-hand side on the real VM.
- 9 unit tests for liveness and register allocation, two of which
  (`the_loop_carried_phi_is_live_across_the_back_edge`,
  `a_value_dead_after_its_own_block_is_not_live_out`) caught two more
  real, independent liveness-analysis bugs during development: phi
  operands were being attributed as "used" in the phi's own block
  instead of the specific predecessor that supplies them (leaking
  liveness backward through a loop's back edge, indefinitely far), and
  a value both defined and consumed within one block was being treated
  as live-in to that block (the classic "upward-exposed use" distinction,
  initially missing). Both fixed and covered by regression tests.
- `compiled_and_interpreted_results_always_agree`: 200 randomly
  generated well-typed programs, each compiled and run on the real VM
  and separately run through `ir::run`'s independent reference
  interpreter, checked for exact agreement — an early instance of what
  ticket 006 will generalize into the project's full differential
  testing story.

## Consequences

- Ticket 004 closes. `chakobsa` -> `mentat` is a real, working,
  end-to-end pipeline: source text compiles to bytecode that runs on an
  independently-developed VM in a separate repository.
- Ticket 005 (`chakobsac` CLI) can build directly on `compile_module`
  and the driver-block pattern these tests already establish.
- Ticket 006's differential testing has a running start: this ticket's
  own `differential.rs` is the shape that ticket needs to generalize
  (larger generated programs, functions/recursion, benchmarks).
