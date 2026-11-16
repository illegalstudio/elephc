---
title: Preserve implicit null from typed array reads at lazy merges
author: Guillaumeloulier
date: 2026-10-16
branch: fix/1836-nullable-array-merges
cost: { minutes: 0, tokens: 0 }
touches:
  - src/ir_lower/expr/assoc_array_literals.rs
  - src/ir_lower/expr/array_merge_values.rs
  - src/ir_lower/expr/array_access.rs
  - src/ir_lower/stmt/return_coercions.rs
  - src/codegen/lower_inst/arrays.rs
  - src/codegen/lower_inst/hashes.rs
---

## Decided
A typed PHP array describes hits, not the complete value of a read. Include implicit null when selecting lazy merge/literal storage. Keep object/container reads concrete; at lazy scalar/callable merge boundaries, request a fresh boxed read through the shared metadata-aware getter. It handles both indexed and hash layouts on both architectures, with warnings and receiver/key side effects once. Explicit casts and string offsets keep ordinary lowering.

Nullable object/callable merge cells returned through a concrete payload ABI must use an owning MixedUnbox before releasing the old owning cell. The payload owns a separate reference; do not retain it twice.

## Tried and ruled out
Boxing an already-converted raw scalar loses missing-value provenance: warned float reads intentionally yield zero and callable misses otherwise become tag-10 boxes containing a sentinel. Widening every checker merge arm to Mixed rejects valid nominal object returns. Keep this representation decision at EIR merge/read boundaries instead.

## Watch out
The ordinary warned float getter has an existing zero-value contract; preserve it outside merge contexts. Runtime null, output, warning counts and clean heap are separate checks. A clean heap alone did not catch the original false-object crash. Freeze each compiler library/CLI before shared Cargo-target writers replace artifacts; rerun after the final source change and rebase.
