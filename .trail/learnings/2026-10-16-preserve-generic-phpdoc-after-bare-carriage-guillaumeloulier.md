---
title: Preserve generic PHPDoc after bare carriage-return comments
author: Guillaumeloulier
date: 2026-10-16
cost: { minutes: 0, tokens: 0 }
touches:
  - src/docblock.rs
  - src/docblock/bindings.rs
  - src/lexer/scan.rs
  - docs/internals/the-lexer.md
---

## Decided

PHPDoc gap recovery must end // and # comments at either CR or LF, matching the lexer. A bare CR before a real annotation previously swallowed @template and its type bound. Use the same two terminators in last_docblock before associating annotations with declaration tokens.

## Tried and ruled out

A declaration being visible to the parser does not prove its annotations survived. The pre-fix compiler accepted identity(42) despite @template T of Entity after either comment kind with bare CR; LF and CRLF controls rejected it. The regression must inspect the AST template bound and parameter/return annotations, then verify the checker rejects invalid arguments and accepts a valid Entity.

## Watch out

Keep apparent docblocks inside ordinary line comments and strings ignored. Test both comment kinds, all three line endings, comments before and after the annotation, and non-ASCII gap text. The new regression fails before the fix; all 29 docblock unit cases and 19 inline-HTML codegen cases pass afterwards, with the latter replayed in both optimizer modes. No new runtime ABI or target-specific emitter behavior is introduced.
