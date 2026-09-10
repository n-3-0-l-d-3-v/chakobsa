---
status: open
phase: 3
---

# 004 — Code generation: lower typed SSA to mentat::Program

Lower the IR (ticket 002/003) to mentat's `Program`/`Block`/
`Instruction` — the first real cross-repo integration point in the
ecosystem (`chakobsa` depending on `mentat`'s `isa` crate as a path/git
dependency).

## Scope
- SSA value -> physical register assignment via linear-scan register
  allocation, bounded by mentat's `NUM_REGISTERS = 32` — spilling to
  memory (mentat's `Load`/`Store`, base-register + immediate offset)
  once live SSA values at a program point exceed the physical register
  count. This is a genuine, not hypothetical, consequence of mentat's
  own "bounded physical registers" constraint reaching into this repo.
- Phi node elimination: insert copies (mentat `Mov`) at the end of each
  predecessor block for each phi in a successor, the standard
  out-of-SSA lowering step.
- IR basic blocks map onto mentat blocks; IR terminators
  (`Jump`/`Branch`/`Return`) map onto mentat's `Jmp`/`Jz`+`Jmp`/`Ret` (a
  `Branch` needs two mentat blocks — mentat has no fallthrough-encoding
  three-way branch).
- Function calls: mentat's `Call`/`Ret` plus an explicit calling
  convention for this repo (argument registers, return-value register,
  caller/callee-saved discipline) — documented in an ADR since mentat's
  own ISA doesn't prescribe one.
- Validate every generated `mentat::Program` with `Program::validate`
  before ever handing it to the VM — codegen bugs must fail loudly at
  the validation boundary, not as a VM trap deep into execution.

Not started. Depends on ticket 003 (parser/IR) for anything nontrivial
to compile.
