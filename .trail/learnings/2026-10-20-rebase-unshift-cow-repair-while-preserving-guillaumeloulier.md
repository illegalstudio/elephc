---
title: Rebase unshift COW repair while preserving both GC test modules
author: Guillaumeloulier
date: 2026-10-20
branch: fix/unshift-reference-cow
---

## Decided
PR #1876 had 180 green checks at 69f2aa0ef61a46c27939d9bcba4d5e643a44acc2, but GitHub reported a merge conflict against current main. Rebase the same fork branch on fetched main 2fe9b2a6e66510d554b6be34fb4ae50ee57387ee. Preserve the new gc_incoming_cache test registration from main and the existing unshift_reference_cow registration, each once.

The ownership repair is unchanged: give a RefCell receiver a separate helper owner before consuming packed array_unshift COW. The only manual rebase resolution was additive test-module wiring. Keep this small fix separate from issue1815 property receiver work.

## Tried and ruled out
Old-head green checks do not validate a rebased artifact or establish mergeability. The rebuilt compiler/library and six original COW/boxed-unshift regressions pass in both IR modes. Both COW fixtures match PHP with clean heaps on Linux ARM64 and x86_64. Their generated code assembles on all five supported targets (ten checks); the iOS library functions contain actual ensure-unique calls, so omitted top-level statements cannot stand in for coverage.

Trail check's five conflicts repeat the original lexical matches: subsequent/split/store and unshift/array/twice describe the removed double decrement, while test/twice concerns an unrelated old strict-types wiring error. Neither owner removal nor duplicate module loading is restored. Existing learnings remain immutable; the commit records this reconciliation.

## Watch out
Freeze Cargo-selected root artifacts and their matching workspace dependencies before another worktree build. Keep all own code, worktrees, temporary files and evidence inside Elephc. The four Linux runs are focused executable evidence; iOS coverage is assembly, not device execution. After publication, require CI against the new exact head, and leave the ready PR ready.
