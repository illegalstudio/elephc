---
title: Validate live unshift receivers before ownership and reindex empty prepends
author: Guillaumeloulier
date: 2026-10-20
branch: fix/1817-ci-refresh-20261017
touches:
  - src/codegen/lower_inst/builtins/arrays/boxed_unshift.rs
  - src/codegen/lower_inst/builtins/arrays/unshift.rs
  - tests/codegen/runtime_gc/boxed_array_unshift.rs
  - examples/mixed_hash_array_builtins/.gitignore
  - examples/mixed_hash_array_builtins/main.php
---

## Decided
PR #1817 review PRRT_kwDORs6r_s6uSQbB is valid at published c796ce44bba46a4c2c7a0988ef43212046f29295: replacing a concrete hash receiver with a missing nested-hash value during argument evaluation produced SIGSEGV with both IR modes. PHP emits its missing-key warning and a catchable TypeError.

Resolve the existing receiver place, reload it after argument evaluation, and validate that current value before prefix allocation or consuming COW. Apply the same order to boxed and concrete hash paths. This also accepts a formerly null receiver made valid by a later argument. Preserve source evaluation order and ordinary alias/array-parameter behavior.

The former count-only no-value hash branch was semantically wrong: PHP still renumbers integer keys and resets append history. A full hash_slice with offset zero, omitted length and preserve_keys false reuses the existing runtime ownership contract while preserving typed value storage and string keys. Publish the replacement before releasing the old table and optional prefix. Keep copied arrays and their append history unchanged.

The original PR implementation was rebased onto main 09c634061ae4b68335066fcb56d6764e26a024b8 after the testing/FFI catalog changes landed. Rebuild the compiler, library and all nine relevant bridge archives together; freeze matching workspace dependencies with their Cargo-selected root artifacts. Earlier successful results on base2fe are interim evidence, not signoff for the new base.

The refreshed source passes 87 focused native tests per IR mode (70 feature plus 17 harness), both aliased-superglobal web tests per mode, 32 Linux executable PHP comparisons with clean heaps, and 80 assembly witnesses over sixteen scenarios on all five supported targets. The CLI example matches PHP in both modes. iOS witnesses contain real hash-slice/unshift calls inside library functions; they are not device execution.

## Tried and ruled out
Checking the pre-argument SSA snapshot, or validating before reloading the current place, cannot prove that the mutation target is still an array. Keeping the zero-value fast count does not implement numeric-key renumbering. Routing typed hashes through the boxed merge would change their value representation; the full typed slice avoids that mismatch.

## Watch out
The earlier learning claiming null receivers were rejected before allocation missed this stale-snapshot ordering. Keep it immutable; this follow-up corrects the invariant and adds direct regressions for the missing case.

Raw nullable-hash gettype/strict-comparison/isset/JSON defects reproduce on the current-main control without unshift. is_null correctly observes the raw null sentinel, so the new state checks use that direct PHP predicate without weakening the required null state. The standalone array teardown destructor-order difference also reproduces without unshift; object-owner tests remove last-copy entries explicitly to verify exact lifetime and order. Both main-only families are preserved as separate follow-ups.

Array-to-scalar retyping controls and arbitrary Mixed receiver replacements remain rejected by the existing checker; do not relax those unrelated boundaries as part of this crash repair. Entry-reference preservation (#1814) and property receiver capture (#1815/#1877) stay separate.

Verify actual iOS library-function call sites, not omitted top-level statements, and distinguish assembly checks from device execution. Preserve missing-key warning text/count in Linux PHP comparisons; strip only source-location suffixes and heap-debug bookkeeping when comparing diagnostics.
