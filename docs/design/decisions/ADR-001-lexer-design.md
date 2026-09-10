# ADR-001: Lexer — fail-fast, single-pass, keyword lookup by full match

## Status
Accepted

## Context

Ticket 001 needed to decide two things before any code got written:
whether a lexical error should abort the whole lex or attempt
error-recovery (skip the bad byte, keep going, collect multiple errors —
the way a production compiler front end usually behaves for a better
single-invocation error report), and how keywords get recognized (a
hand-rolled trie/DFA is the "fast compiler" answer; a plain hash lookup
after scanning a whole identifier is the simple one).

## Decision

**Fail-fast**: `lex()` returns the first `LexError` it hits and stops.
No error-recovery mode, no multi-error collection. This matches every
other untrusted-input boundary in the wider ecosystem (sietch's
`Segment::recover`, mentat's `Program::validate`/`decode`) — they all
report the first problem as a typed error rather than trying to produce
a "best effort" partial result. A future ticket can add recovery if a
real multi-error-report UX becomes a stated goal; nothing here forecloses
it, since `LexError` already carries a `Span`.

**Keywords via full-match lookup, not a trie**: scan the whole
identifier first (letters/digits/underscore), then check it against a
plain `match` over keyword strings. A trie/DFA would recognize a keyword
without ever building the substring, which matters for a compiler
lexing gigabytes of source — not a concern here (`docs/DEFINITION_OF_DONE.md`'s
"real workload, not a toy" bar is about the language semantics and
codegen being real, not about lexer throughput at a scale this project
will never see). The full-match approach is also what correctly handles
"iffy is an identifier, not if followed by fy" for free — the whole
identifier is always scanned to its natural end before any keyword
comparison happens, so there's no risk of the trie approach's classic
bug (stopping early at a keyword-boundary character that turns out to
be the middle of a longer identifier).

## Alternatives Considered
1. Error-recovery lexing (skip-and-continue, collect all errors) —
   rejected for now: more state to test correctly, and no ticket in this
   repo's scope currently needs a multi-error report.
2. Trie/DFA-based keyword matching — rejected: no measured throughput
   requirement justifies the complexity; `docs/DEFINITION_OF_DONE.md`'s
   benchmarking requirement will catch it later if lexer throughput ever
   turns out to matter for a downstream ticket.

## Consequences

- A single malformed byte anywhere in a source file fails the whole
  compile with one error, not a batch — acceptable for this project's
  current scope (a single-file CLI compiler, ticket 005), revisit if a
  language-server-style "report everything wrong" use case is ever
  added.
- Adding a new keyword is a one-line change to the `keyword()` match and
  needs no separate automaton regeneration step.
