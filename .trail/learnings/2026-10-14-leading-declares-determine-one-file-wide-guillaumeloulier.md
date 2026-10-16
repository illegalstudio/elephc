---
title: Leading declares determine one file-wide strict mode
author: Guillaumeloulier
date: 2026-10-14
branch: review/1788-declare-statement-scanner
cost: { minutes: 0, tokens: 0 }
touches:
  - src/parser/stmt/declare_scan.rs
  - src/parser/stmt/declare.rs
  - src/parser/stmt/mod.rs
  - src/source.rs
  - tests/parser_tests/declare_order.rs
  - tests/error_tests/declare_order.rs
  - tests/codegen/types/strict_types.rs
  - tests/parser_tests.rs
  - tests/error_tests.rs
---

## Decided
For #839 / PR #1788, every accepted leading strict_types directive contributes to one physical-file flag: any value 1 enables strict typing for the whole file, and a later 0 cannot clear it. Prescan all leading declare statements before creating AST nodes. Keep the declaration update monotonic; ParseModeGuard still replaces/restores the per-file profile, preserving coercive included files and callers.

Keep boundary scanning separate from directive parsing in src/parser/stmt/declare_scan.rs. A single statement includes its elseif/else, catch/finally and do/while tails. Alternative syntax consumes complete nested statements through the matching closing keyword, including switch labels whose expressions contain ternary colons.

A closing brace ends a named declaration, but does not end a match or closure expression followed by an operator. Keep scanning those expressions through their semicolon. Skip attribute groups without constructing AST nodes, and use the parser's name-part predicate to distinguish contextual enum declarations from enum expressions.

## Tried and ruled out
Stopping at the first strict_types value and replacing the flag on each directive made 1 followed by 0 coercive, and failed to stamp a preceding ticks body when a later directive enabled strictness. Stopping every single statement at its first semicolon or closing brace lost control tails. Treating every top-level closing brace as a boundary also rejected valid match-plus and closure-coalesce statements.

## Watch out
Do not speculatively parse prefix bodies into AST to find boundaries: parsing has profile and anonymous-class bookkeeping effects. Keep the real statement parser authoritative for syntax diagnostics and reject a strict directive after a separate real statement.

Validated PHP-oracle witnesses, 9 parser tests, 22 diagnostic tests, 28 native codegen tests, 31 compiler lexer tests and 38 Magician lexer tests. The final native harness also passed 17 support tests. The first custom harness run lacked runtime CARGO_BIN_EXE_elephc for three CLI fixtures; after correcting that environment the final full focused harness passed 45/45. These are focused host results, not a full suite or Linux/iOS execution claim. Time/token costs were not measured and metadata is explicitly zero.

Eval fragments containing declare(strict_types=0) fail to parse on both the reviewed baseline and this candidate, unlike PHP. That is a separate eval feature follow-up, not validation of eval directive support by this physical-file fix.
