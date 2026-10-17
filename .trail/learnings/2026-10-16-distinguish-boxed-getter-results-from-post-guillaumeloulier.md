---
title: Distinguish boxed getter results from post-conversion boxing
author: Guillaumeloulier
date: 2026-10-16
branch: fix/1836-nullable-array-merges
cost: { minutes: 0, tokens: 0 }
touches:
  - src/ir_lower/expr/array_access.rs
  - src/codegen/lower_inst/hashes.rs
  - src/ir_lower/stmt/return_coercions.rs
---

## Decided
For issue #1836, a merge-bound scalar getter returns a fresh Mixed cell directly from the runtime metadata reader. Typed object/container reads stay concrete. A nullable object projection acquires its payload through MixedUnbox and retires the owning merge cell.

## Tried and ruled out
Trail check reported five lexical conflicts against the Oct 15 Synthetic-group learning and the Oct 16 array-read learning. They are false matches on shared words in comments and docs. This patch does not alter Synthetic groups. The scalar read is boxed before conversion, exactly as the new learning requires; the object return projection is a separate ownership operation. All 19 PHP value/warning/heap fixtures passed in both optimizer modes before this clarification.

## Watch out
Review behavior and the actual opcode/result type, not the lexical box/miss/scalar overlap. The consuming-string-append and scalar-closure-loop overlaps were already rebased from origin/main b56a0c8cf3. The types module overlap is only a separate test registration. Keep full cross-target validation as a distinct gate.
