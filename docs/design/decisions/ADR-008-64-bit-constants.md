# ADR-008: Materialising 64-bit constants

## Status
Accepted (follow-up to ADR-004, done during release preparation)

## Context
chakobsa's only integer type is `i64`, but mentat's instruction format has a
32-bit immediate field (`LoadI dst, imm` sign-extends an `i32`). ADR-004
rejected any constant outside the `i32` range with a typed
`CodegenError::ConstantTooLarge`: never wrong code, but a program such as
`return 5000000000;` did not compile, and the ARRAKIS README had to say
"constants must fit in 32 bits".

The question was whether this could be fixed without destabilising the
compiler or the VM. It can: no new opcode, no change to mentat, no change to
the IR or the register allocator.

## Decision
Lower a constant outside the `i32` range to five instructions:

```
LoadI dst, hi          ; sign-extended
LoadI SCRATCH_2, 32
Shl   dst, dst, SCRATCH_2
LoadI SCRATCH_2, lo    ; sign-extended
Add   dst, dst, SCRATCH_2
```

with `lo = n as i32` (the low 32 bits read as signed) and
`hi = (n - lo) >> 32`. Because `lo` is sign-extended, `hi` absorbs the borrow
and always fits in an `i32`, and since mentat's `Shl` and `Add` wrap (as the
`ir` reference interpreter does), `(hi << 32) + lo == n` for every `i64`,
including `i64::MAX` and `i64::MIN`. Constants inside the `i32` range still
use one `LoadI`, so no existing program changes.

`SCRATCH_2` is safe as the temporary: a constant has no operands, so nothing
is being read through it, and the destination is either an allocated
register or `SCRATCH_1` (when the value is spilled), never `SCRATCH_2`.

Rejected: a constant pool in memory (needs a data section mentat does not
have), and a new `LoadHi` opcode (changes the ISA of a finished subsystem for
a compiler convenience).

`CodegenError::ConstantTooLarge` is removed: it can no longer happen. The
lexer still rejects a literal above `i64::MAX` (`IntegerOverflow`), so
`i64::MIN` is written `-9223372036854775807 - 1`, as in most languages.

## Tests
- Property test `split_i64_reassembles_every_value` (any `i64`) plus explicit
  edges, in `lower.rs`.
- `constants_wider_than_32_bits_are_materialised_exactly`: twelve values
  either side of each `i32` boundary, `u32::MAX`, `2^32`, both ends of `i64`,
  run on the real VM.
- `spilled_wide_constants_survive_register_pressure`: thirty wide constants
  live at once, more than the 27 allocatable registers, so the spilled path
  (`SCRATCH_1` destination) is exercised.
- `wide_constants_in_a_loop`.
- The differential generator (ADR-006) now emits a wide constant as about one
  atom in seventeen, so random programs compare compiled and interpreted
  results with full-range constants too.

Mutation check: with `hi = n >> 32` (forgetting the borrow from a negative
`lo`), the differential test, the boundary test and the spill test all fail;
the saved proptest regression case is the shrunk counterexample.

## Consequences
- Any `i64` literal compiles. A wide constant costs five instructions
  instead of one; no benchmark program uses one, so ADR-006's numbers are
  unchanged.
