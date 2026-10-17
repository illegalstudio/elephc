---
title: Keep executable snapshots for reviewed property cleanup validation
author: Guillaumeloulier
date: 2026-10-16
branch: review/1823-1834-finalize
cost: { minutes: 0, tokens: 0 }
touches:
  - tests/codegen/runtime_gc/property_source_destructor_owners.rs
  - ".plans/issue-1643-nullable-property-review-followup.md"
---

## Decided

Retain statically linked compiler, focused-probe, and runtime-emitter executables before reusing a shared Cargo target for another worktree. A frozen root rlib is useful provenance, but relinking it also depends on the matching dependency artifacts. Keep source, compiler hash, target, optimizer mode, full output, and heap result attached to each validation receipt.

## Tried and ruled out

Relinking an earlier frozen root rlib after other worktrees rebuilt shared dependencies failed with E0463. The already-linked compiler still replayed the exact published failures. Do not rebuild a supposed baseline implicitly or substitute another branch's current target artifact. The PR #1823 controls initially used a main-baseline CLI for an existing structural target test; replay that control with the final reviewed CLI before describing it as final-target evidence.

## Watch out

Trail check reported lexical matches between the #1823 plan, documentation, source comments, tests, and warnings about pin ordering. The actual change detaches only the source before its release, leaves the receiver registered until source destruction returns, and checks PHP output plus heap cleanup. Other matches concern unrelated callable/null-box/conditional-function lessons or historical external artifact paths; this continuation uses only paths inside Elephc. Preserve those explanations in the commit and PR rather than removing useful documentation to satisfy keyword matching.
