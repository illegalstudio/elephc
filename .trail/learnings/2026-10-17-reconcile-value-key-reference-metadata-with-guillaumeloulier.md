---
title: Reconcile value-key reference metadata with Trail lexical matches
author: Guillaumeloulier
date: 2026-10-17
branch: fix/eval-array-reference-key-lease
touches:
  - ".plans/eval-array-reference-key-lease.md"
  - crates/elephc-magician/src/context/reference_metadata.rs
  - crates/elephc-magician/src/interpreter/array_reference_arguments.rs
  - crates/elephc-magician/src/interpreter/array_references.rs
  - crates/elephc-magician/src/interpreter/dynamic_functions.rs
  - crates/elephc-magician/src/interpreter/mod.rs
  - crates/elephc-magician/src/interpreter/persistent_references.rs
  - crates/elephc-magician/src/interpreter/statements/class_resolution.rs
  - crates/elephc-magician/src/interpreter/statements/instance_property_access.rs
  - crates/elephc-magician/src/interpreter/statements/reference_writeback.rs
---

## Decided

Retain the reference-key fix after reviewing all20Trail check matches. PersistentArrayElement stores EvalArrayReferenceKey integer/byte-vector values, not RuntimeCellHandle indices. Native key handles exist only while evaluating or materializing an operation and are explicitly retired. Reads using the native protected adapter defer intermediate cleanup through its existing owner list; nested write-back retires its replacement clone after publishing it.

## Tried and ruled out

The lexical checker matched the rejected native-key approach against prose describing its removal, its replacement enum and the matching regression tests. Other matches refer to unrelated Synthetic normalization, class-layout masks, call binders or old iOS witnesses. Those implementations are untouched. The key snapshot is exercised through actual reads/writes and PHP output, with clean heaps; it is not an absence-only or counter-only test.

## Watch out

Record the20matches and this source-based reconciliation in the commit/PR. Do not edit prior learnings to silence the checker. Mainframe keys and property-layout rows are different concepts; the parent here is an array storage target, not a class declaration owner. No iOS executable evidence is claimed: its existing pcntl build blocker remains recorded separately. Keep independent builtin iterator-leak reproductions and the original strict1568fixtures as outstanding work rather than counting them as clean.
