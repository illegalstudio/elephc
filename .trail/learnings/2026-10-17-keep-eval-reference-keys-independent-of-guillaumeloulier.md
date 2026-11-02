---
title: Keep eval reference keys independent of native key leases
author: Guillaumeloulier
date: 2026-10-17
branch: fix/eval-array-reference-key-lease
touches:
  - crates/elephc-magician/src/context/reference_metadata.rs
  - crates/elephc-magician/src/interpreter/array_references.rs
  - crates/elephc-magician/src/interpreter/dynamic_functions.rs
  - crates/elephc-magician/src/interpreter/mod.rs
  - crates/elephc-magician/src/interpreter/persistent_references.rs
  - crates/elephc-magician/src/interpreter/statements/class_resolution.rs
  - crates/elephc-magician/src/interpreter/statements/instance_property_access.rs
  - crates/elephc-magician/src/interpreter/statements/reference_writeback.rs
  - crates/elephc-magician/src/interpreter/tests/mod.rs
  - docs/internals/memory-model.md
---

## Decided

Array-element reference targets created while evaluating eval arguments now store EvalArrayReferenceKey integer/byte-string values. The original native offset cell is an evaluation lease, retired after the element is read. PersistentArrayElement avoids retaining a dangling borrowed key or leaking an owned key through cloned alias metadata. Protected native readers defer materialized key and receiver owners through their existing cleanup list; ordinary reads and writes retire short-lived key cells. Nested write-back owns the current read and replacement clone until publication, then releases both.

## Tried and ruled out

Leaving RuntimeCellHandle indices in persistent targets leaked40bytes for integer keys and88bytes for string keys, and rebinding the index could change the referenced element or reuse freed storage. Unit-only checks passed while native mutation cases exposed an additional unretired replacement clone; retain/release the read and write-back owners explicitly. Preserve candidate1 failures and do not treat correct output as proof of balanced ownership.

## Watch out

All108focused interpreter tests and18native reference executions passed, with complete PHP output and clean heaps. Nine oracles also agree betweenPHP8.2.34and8.5. Separate eval sort/implode/array_keys iterator leaks reproduce without references; their original programs remain recorded for a dedicated fix. The binary-key witness reads and mutates the exact binary key without routing through independently leaking array_keys. All seven strict PR1568 review fixtures remain unchanged. Linux type checks and sharded CI are separate from native macOS execution; the existing pcntl iOS compilation failure is not a passing bridge gate.
