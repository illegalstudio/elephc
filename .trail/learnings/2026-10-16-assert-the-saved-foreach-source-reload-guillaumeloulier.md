---
title: Assert the saved foreach source reload after the kind comparison
author: Guillaumeloulier
date: 2026-10-16
cost: { minutes: 0, tokens: 0 }
touches:
  - tests/codegen/arrays/foreach_by_ref_growth.rs
---

## Decided

Pin the actual source reload after the heap-kind comparison and before hash-tail resynchronization. Match the memory-load instruction used for the heap-kind probe, substitute the resync helper's own argument register, and verify the same frame-slot address setup when the load uses a scratch address register.

## Tried and ruled out

Searching for `x0,` in the region also matches `cmp x0, #3`, so the old assertion passed after deleting the table reload from real ARM assembly. The corrected test rejects both a removed reload and a different source slot on every supported target. The old compiled assertion still passes the injected missing ARM reload, confirming that this is a test-sensitivity repair.

## Watch out

Heap-kind arguments use x0/rax; resync uses x0/rdi. Comparing only registers misses the overwritten return value, and comparing only a scratch memory operand misses a changed frame address. Keep the comparison boundary and frame provenance in the assertion. iOS probes put the loop in a named function and emit staticlib assembly. This follow-up changes tests only; tail deletion/recreation is still the separate issue1807.
