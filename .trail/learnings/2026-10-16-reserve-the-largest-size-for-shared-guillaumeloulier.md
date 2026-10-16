---
title: Reserve the largest size for shared common symbols
author: Guillaumeloulier
date: 2026-10-16
cost: { minutes: 0, tokens: 0 }
touches:
  - src/codegen_support/data_section.rs
  - tests/codegen/casts_and_constants/constants.rs
---

## Decided

Common storage declarations must reserve the maximum requested size for a symbol, regardless of emission order. DataSection records each common entry's index so add_comm can grow its size without changing the label, first-seen order or deduplication. Never shrink an earlier wide reservation.

## Tried and ruled out

The published compiler emitted an integer define inside a function before a main-script string const and kept an eight-byte slot despite the later sixteen-byte store. A failing test of the actual DataSection module and frozen CLI assembly reproduced this. First-declaration-wins capacity is unsafe even when runtime duplicate guards preserve the first value.

## Watch out

Test narrow-then-wide and wide-then-narrow reservations on every supported target, plus a source regression spanning function/main definitions. Assert the emitted capacity before executing a formerly unsafe fixture. Derive assembly symbol names through the canonical helpers: underscores are escaped in the actual symbol and raw PHP spelling is not the assembler name. Keep same-request duplicate-warning and reused-web-worker guard tests as separate controls.
