---
status: done
phase: 3
---

# 004 — Code generation: lower typed SSA to mentat::Program

Lower the IR (ticket 002/003) to mentat's `Program`/`Block`/
`Instruction` — the first real cross-repo integration point in the
ecosystem (`chakobsa` depending on `mentat`'s `isa` crate as a `git`
dependency).

## Scope
- [x] SSA value -> physical register assignment via greedy graph
      coloring over a block-granularity interference graph, bounded by
      mentat's `NUM_REGISTERS = 32` — spilling to memory (mentat's
      `Load`/`Store`, `ZERO_REG`-relative absolute addressing) once live
      SSA values at a program point exceed the physical register count.
      This is a genuine, not hypothetical, consequence of mentat's own
      "bounded physical registers" constraint reaching into this repo.
- [x] Phi node elimination: `parallel_move` inserts copies at the end of
      each predecessor block for each phi in a successor — staged
      through scratch memory so aliased moves (a swap) can never
      clobber a still-unread source.
- [x] IR basic blocks map onto mentat blocks; IR terminators
      (`Jump`/`Branch`/`Return`) map onto mentat's `Jmp`/`Jz`+thunk/
      `Ret`. A `Branch` needs two mentat blocks — mentat has no
      fallthrough-encoding three-way branch (`Jz`/`Jnz` name only one
      target; the thunk block supplies the other explicitly, correct
      regardless of physical block layout elsewhere).
- [x] Function calls: mentat's `Call`/`Ret` plus an explicit calling
      convention (positional argument registers, `RET_REG` for the
      return value, and a genuine software call stack for caller-live
      register save/restore, since mentat's own call stack only tracks
      return addresses) — documented in ADR-004 since mentat's own ISA
      doesn't prescribe one.
- [x] Validate every generated `mentat::Program` with `Program::validate`
      before ever handing it to the VM — codegen bugs fail loudly at the
      validation boundary, not as a VM trap deep into execution.

Four real bugs surfaced during this ticket's own testing, all fixed and
covered by regression tests rather than papered over — see
`docs/design/decisions/ADR-004-codegen-and-calling-convention.md` for
the full account:
1. A naive sequential `Mov`-per-argument call-argument-passing scheme
   corrupts a call like `sub(b, a)` when `a`/`b` alias the destination
   registers — fixed with `parallel_move`.
2. mentat's `Not` opcode is bitwise (`!1 == -2`), not logical negation —
   fixed by lowering `not x` to `x == 0`.
3. Calling any function — especially recursively — silently clobbered a
   caller's own still-needed values, since mentat's call stack tracks
   only return addresses, never registers. Fixed with a genuine software
   call stack (`STACK_PTR`) giving each active invocation, including
   recursive ones, non-overlapping save storage.
4. Two independent liveness-analysis bugs: a `Phi`'s incoming values
   were attributed as "used" in the phi's own block rather than the
   specific predecessor supplying them, leaking liveness backward
   through a loop's back edge indefinitely; and a value both defined and
   consumed within one block was incorrectly treated as live-in to that
   block (the classic upward-exposed-use distinction). Both caught by
   dedicated `liveness.rs` unit tests before they could cause a
   register-allocation bug downstream.

13 end-to-end tests run compiled output on mentat's **real VM** (not the
interpreter), 9 unit tests cover liveness/regalloc directly, and a
200-case differential property test
(`compiled_and_interpreted_results_always_agree`) confirms compiled-and-
VM-executed results match `ir::run`'s independent reference interpreter
for arbitrary generated programs — an early instance of ticket 006's
full differential-testing story.
