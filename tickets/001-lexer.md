---
status: done
phase: 3
---

# 001 — Lexer

A flat token stream from source text — the one and only tokenization
pass in this pipeline (see `docs/design/LANGUAGE.md`: no separate token
tree, no re-scanning downstream).

## Acceptance criteria
- [x] Full token set for the v1 surface language: literals (`Int`,
      `Ident`, `true`/`false`), keywords (`let fn if else while return
      and or not i64 bool`), operators (`+ - * / % == != < <= > >= =`),
      punctuation (`( ) { } , ; : ->`), with a trailing `Eof`.
- [x] Two-character operators (`== != <= >= ->`) greedily preferred over
      their one-character prefixes.
- [x] Comments (`//` to end of line) and whitespace are skipped, never
      produce tokens.
- [x] Keyword-prefixed identifiers (`iffy`, `letter`) lex as identifiers,
      not as a keyword plus a truncated remainder.
- [x] Malformed input (an unexpected character, a lone `!` not followed
      by `=`, an integer literal that overflows `i64`) is always a typed
      `LexError`, never a panic — proven by a property test over
      completely arbitrary byte input, not just hand-picked bad cases.
- [x] Every token carries an accurate byte-offset `Span` into the
      source.
- [x] Property test: an arbitrary sequence of well-formed tokens,
      rendered back to source text and re-lexed, must produce exactly
      the same token-kind sequence it was built from.

See `docs/design/decisions/ADR-001-lexer-design.md` for the design
choices (fail-fast on the first bad token rather than error-recovery
mode, and why keywords are matched by full-identifier lookup rather than
a trie/automaton).
