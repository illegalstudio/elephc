---
title: "Prelude notices report the caller's file and line"
author: Guillaume
date: 2026-10-26
branch: feat/random-extension
cost: { minutes: 76, tokens: 202531 }
touches:
  - AGENTS.md
  - CLAUDE.md
  - docs/php/random.md
  - src/codegen/lower_inst/callables.rs
  - src/codegen/lower_inst/core_builtins/backtrace.rs
  - src/codegen/lower_inst/core_builtins/handlers.rs
  - src/ir_passes/inline.rs
  - src/ir_passes/tests/inline_test.rs
  - tests/codegen/random/functions.rs
  - ".beads/.auto-import-issues.jsonl"
---

## Decided
- `trigger_error` lowered from compiler-generated code (injected prelude spans) now reads `_php_diagnostic_file/line` instead of its baked operands. Those globals are what `prepare_call_site` publishes for the entering call, so a prelude-implemented function behaves like a php-src internal function: the notice names the user's call site.
- `prepare_call_site` skips republishing the `_php_diagnostic_*` location for instructions whose span is not from source. Prelude bodies leave the caller's publication intact; source-written code still overwrites it per instruction.
- The EIR small-function inliner refuses a callee containing a `TriggerError` at a compiler-generated span (`callee_reports_at_an_injected_span`). Splicing removed the `Call` that publishes the location, which made `lcg_value()` report the enclosing helper's line (or `on line 0` at top level).

## Tried and ruled out
- Reading the diagnostic globals inside `lower_trigger_error` without the publication skip: the callee's own `TriggerError` instruction republished the synthetic prelude line first.
- Re-stamping inlined prelude spans with the call site: the baked file/line operands still carried the synthetic line, and `caller_site` is derived from the instruction span, so the two disagreed.
- Storing globals into the file/line operand values: constant values can be recipe-rematerialized (no stack slot), so the store is unobservable.

## Watch out
- Only `prepare_call_site` writers should publish `_php_diagnostic_*`; a new publisher for generated code will break internal-function attribution.
- `Span::is_from_source()` is the predicate for "compiler generated"; `line == 0` (dummy) also skips publication, which is the desired fallback.
- The inliner veto is deliberately narrow (TriggerError + non-source span). User functions that call `trigger_error` at a real span still inline.
