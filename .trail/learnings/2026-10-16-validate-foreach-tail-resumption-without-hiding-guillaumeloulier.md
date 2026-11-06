---
title: Validate foreach tail resumption without hiding deleted-key followups
author: Guillaumeloulier
date: 2026-10-16
cost: { minutes: 0, tokens: 0 }
touches:
  - src/codegen/lower_inst/iterators.rs
  - src/codegen_support/runtime/arrays/hash_entry_reference.rs
  - tests/codegen/arrays/foreach_by_ref_growth.rs
  - tests/codegen/runtime_gc/by_ref_foreach_reference_cells.rs
---

## Decided

A live tail-key anchor resumes ordinary by-reference foreach appends across rehash. Keep the separate deleted-tail case tracked as issue #1807. An exact main build at d3542732744e46dbe623951d53ee0ddba4f2498e and the rebased PR1811 build both miss deleted numeric tails, recreated numeric keys, and deleted string tails; these are not new regressions introduced by PR1811.

## Tried and ruled out

Do not use executable top-level statements as an iOS static-library assembly witness: that output mode omits them. Put the foreach in a named function and emit staticlib on the two iOS targets. The caller-register regression then checks the heap-kind and successor-resync arguments on all five supported targets.

## Watch out

A clean heap does not establish iteration correctness: all three failing deletion witnesses report no leaks while missing output. The focused 19 growth tests and two heap regressions pass with EIR optimization both on and off. Native macOS execution, all-target assembly emission, and complete CI execution remain distinct evidence.
