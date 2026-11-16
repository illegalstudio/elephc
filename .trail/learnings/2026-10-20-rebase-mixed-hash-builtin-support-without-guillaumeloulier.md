---
title: Rebase Mixed hash builtin support without dropping pop history and shift ownership checks
author: Guillaumeloulier
date: 2026-10-20
branch: fix/1817-ci-refresh-20261017
touches:
  - src/codegen/lower_inst/builtins/arrays.rs
  - src/codegen/lower_inst/builtins/arrays/basic.rs
  - src/codegen/lower_inst/builtins/arrays/boxed_mutation.rs
  - src/codegen/lower_inst/builtins/arrays/boxed_unshift.rs
  - src/codegen/lower_inst/builtins/arrays/shift.rs
  - src/codegen/lower_inst/builtins/arrays/sort_dispatch.rs
  - src/codegen/lower_inst/builtins/arrays/unshift.rs
  - src/codegen_support/runtime/arrays/array_take_boxed.rs
  - tests/codegen/arrays/assoc_helpers.rs
  - tests/codegen/runtime_gc.rs
---

## Decided

Refresh PR #1817 from its existing fork branch onto the current main before publishing. Its old lockfile selected rustls 0.23.45, while main already selected the 0.23.46 security fix. Keep the advisory gate enabled and update the existing PR with an exact remote lease.

When combining the array_take_boxed tests, retain both main's pop append-history assertions and the Mixed-hash shift ownership assertions. The pop paths rewind the append counter; shift rebuilds and renumbers its surviving numeric keys. Retain removed values before unlinking their entry.

## Tried and ruled out

A retry against an obsolete dependency lockfile cannot repair its vulnerable dependency selection. Choosing only one side of the adjacent emitter-test conflict discards a relevant regression.

## Watch out

Rebuild the compiler and native bridge archives from the rebased source. Freeze Cargo-selected artifacts and workspace dependencies before changing worktrees. Focused native GC/array tests and aliased web superglobals run with both EIR optimization modes. Execute Linux ARM64 and x86_64 fixtures; Apple target assembly checks are separate evidence. Preserve warning text and multiplicity when PHP and Elephc route diagnostics to different streams; normalize location suffixes only.

Keep the property receiver follow-up #1815 on PR #1877 and reference-entry preservation #1814 separate from this Mixed-hash implementation. Do not carry forward old full-suite or reviewer claims as evidence for the new head.
