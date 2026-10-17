---
title: Release associative array children in PHP insertion order
author: Guillaumeloulier
date: 2026-10-17
branch: fix/hash-destruction-insertion-order
touches:
  - docs/internals/memory-model.md
  - src/codegen_support/runtime/arrays/hash_free_deep.rs
  - src/codegen_support/runtime/arrays/hash_layout.rs
  - tests/codegen/runtime_gc.rs
  - tests/codegen/runtime_gc/hash_destruction_order.rs
---

## Decided

Associative deep release must follow the stable header head and each entry next link, like foreach and array_keys. Scanning physical buckets changes PHP-observable destructor order after collisions, growth or deletion. Both AArch64 and x86_64 now begin at HEAD_OFFSET24, snapshot ENTRY_NEXT_OFFSET56 before releasing a key/value, and advance through the saved successor. The former capacity stack slot holds that successor; capacity is reloaded from the stable header for an unsigned bound that also recognizes the -1 end sentinel. Frame sizes and protected cleanup/pending-Throwable slots are unchanged.

## Tried and ruled out

Physical bucket scanning produced321forthreeobjects instead ofPHP123; reversing a scan would still fail after growth or reinsertion. Preserve the baseline oracles and old native outputs. Do not test only the first three string keys: deleted/reinserted entries, huge integer keys, COW, nested hashes, a deleted final entry and throwing destructors exercise different order and cleanup edges.

## Watch out

All80program/runtime object combinations assembled across five supported targets, and48actualmacOS/Linuxexecutions matched fullPHPoutput andcleanheaps inbothIRmodes. iOS evidence is compilation/assembly only. Sixteen original-shape macOS runs also pass. Snapshot the next link before nested cleanup, preserve the separate entry allocation until traversal ends, and retain the existing protected child-exception behavior. Keep eval reference-key and builtin-iterator lifetime fixes separate; this change only repairs native hash teardown order.
