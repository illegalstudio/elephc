---
title: Canonicalize raw null receivers before dynamic array getters
author: Guillaumeloulier
date: 2026-10-17
branch: fix/1836-nullable-array-merges
cost: { minutes: 1474, tokens: 11069584 }
touches:
  - docs/internals/the-ir.md
  - src/ir_lower/expr/array_access.rs
  - tests/codegen/runtime_gc/nullable_array_read_owners.rs
  - tests/codegen/types/nullable_array_merge.rs
  - ".plans/issue-1836-nullable-array-merges.md"
  - src/codegen/lower_inst/arrays.rs
  - src/codegen/lower_inst/hashes.rs
  - src/ir_lower/expr/array_merge_values.rs
  - src/ir_lower/expr/assoc_array_literals.rs
  - src/ir_lower/expr/dynamic_spreads.rs
---

## Decided

PR #1859 routes nullable receiver reads through a dynamic getter even for undefined roots whose EIR value is I64 const_null. That ABI requires a boxed Mixed cell. In the proven-null arm, retire the original owner and construct canonical boxed null before calling the shared getter. Pin this canonical owner across offset evaluation and warning delivery so both exceptional exits release it, then detach the non-owning pin after normal getter cleanup.

## Tried and ruled out

Pinning the original nullable value did not make a raw null word into a valid cell. The published09fc compiler crashes the existing isset($neverIndexed["k"]) test on macOS, Linux ARM64 and Linux x86_64; frozen main6459 passes on macOS. The replacement passes the exact CI test and the throwing offset/warning-handler heap regressions with both IR modes. Checking only --check is insufficient: array-valued keys are accepted there but baseline unoptimized EIR codegen rejects them. Those unsupported baseline shapes do not belong in this receiver-ABI regression fixture.

## Watch out

Owned string function results used as hash keys have a separate baseline48-byte leak with IR optimization on. Preserve baseline-owned-string-key.json and fix that on its own branch, not by treating a leaking fixture as acceptance proof. The quiet offset regression uses an int-returning function so it verifies once-only key effects and suppression independently. Keep all original published09fc artifacts, final per-file/artifact hashes and real Linux executions separate from iOS assembly evidence. A rejected literal-unpack null destination already leaks72bytes on main; do not count those known controls as heap-clean.
