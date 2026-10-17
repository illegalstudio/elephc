---
title: Distinguish the unshift ownership repair from lexical Trail conflicts
author: Guillaumeloulier
date: 2026-10-17
branch: fix/unshift-reference-cow
touches:
  - src/codegen/lower_inst/builtins/arrays/unshift.rs
  - tests/codegen/runtime_gc.rs
  - tests/codegen/runtime_gc/unshift_reference_cow.rs
---

## Decided
Keep the explicit extra owner before array_unshift consumes a RefCell receiver. It removes the double decrement described in the preceding learning; it does not restore that behavior. The new unshift_reference_cow module is registered once at the end of tests/codegen/runtime_gc.rs. The focused harness loads its two original tests once, and both pass with EIR optimization on and off and a clean heap. Both PHP fixtures also produce exact reference outputs when executed on Linux ARM64 and x86_64; the Apple target assemblies assemble successfully.

## Tried and ruled out
Trail check's three reported conflicts are lexical false positives. The phrases subsequent split store and unshift array twice matched prose describing the original double decrement. The test twice regression match came from a separate strict-types module-wiring learning, not duplicated registration in this change. Inspecting the actual changed code and the two-test harness results reconciles all three findings. State this justification in the commit and PR rather than changing an existing learning.

## Watch out
Do not infer ownership correctness or duplicate test registration from shared words in comments. Verify the reference counts and complete outputs. Linux execution here used focused generated programs; it is not the full Linux suite. iOS evidence is assembly, not device execution.
