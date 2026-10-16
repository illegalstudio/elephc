---
title: Stop leading-declare scans at complete ifdef blocks
author: Guillaumeloulier
date: 2026-10-16
cost: { minutes: 0, tokens: 0 }
touches:
  - src/parser/stmt/declare_scan.rs
  - tests/parser_tests/declare_order.rs
  - tests/error_tests/declare_order.rs
  - ".config/nextest.toml"
---

## Decided

The leading-declare boundary scan follows parse_ifdef's grammar: one identifier, a balanced braced body, and an optional braced else body. It stops at the closing block without requiring a semicolon, preserving the whole physical file's strict-mode seed and the boundary before a separate real statement.

## Tried and ruled out

The expression-statement fallback scans past an empty ifdef block until the next semicolon. On the exact reviewed compiler, that rejects a valid following strict_types directive but accepts an illegal directive after an intervening echo. Parser and diagnostic regressions fail for those semantic reasons before the repair.

## Watch out

Cover empty, nested, else and malformed blocks, and strictness in an earlier selected branch. ifdef is an Elephc extension, so use the actual parser and define-enabled/disabled executions rather than a PHP syntax oracle. Shared frontend changes do not require target-specific emitter changes. The current macOS CI timeout was a separate all-target unit test: keep its 180-second budget scoped to both `_on_all_targets` and `_on_every_target`, preserving the global 60-second cap.
