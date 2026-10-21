---
title: Refresh hash teardown while retaining current ownership test modules
author: Guillaumeloulier
date: 2026-10-20
branch: fix/hash-destruction-insertion-order
touches:
  - ".plans/hash-destructor-insertion-order.md"
---

## Decided
Refresh PR1874 on pinned current main487cae6abc7d98c1f16bb99042e491bb8a32e27b while preserving both test-module registrations: main's borrowed_numeric and the feature's hash_destruction_order. The hash_free_deep and hash_layout runtime files and the original regression source are byte-identical to the published feature; no destructor traversal or protected-cleanup ABI change was needed.

## Tried and ruled out
Do not resolve the registration conflict by choosing one side: it silently drops either current numeric ownership tests or the destructor-order regressions. Do not describe an old frozen compiler as post-rebase validation. Build and freeze Cargo-selected artifacts and matching workspace dependencies before another worktree uses the common target. Fresh evidence includes 18 GC regressions plus23 harness checks per IR mode, 80 program/runtime assembly pairs and48 actual native executions with complete PHP output and clean heaps. The native matrix uses the original entry-point shape, not a different library wrapper.

## Watch out
Opaque eval cases need per-process heap statistics here, so retain the existing compile-once/per-argv helper rather than stdout-only bundling. iOS evidence remains assembly only. Trail's lexical matches do not restore Synthetic flattening, modify heap-kind/successor-resync contracts, change property-store pins or reintroduce external worktrees. Instruction comments remain complete and aligned. Keep the independent eval reference-key dependency PR1873 separate; combine it with this existing PR and PR1568 only in an internal integration worktree.
