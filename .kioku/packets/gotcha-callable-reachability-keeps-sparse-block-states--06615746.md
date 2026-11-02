---
id: gotcha-callable-reachability-keeps-sparse-block-states--06615746
type: gotcha
title: "Callable reachability keeps sparse block states: absent means unknown, and the dense form cost 757 MB for one method"
description: "The per-block local state lists only locals with a finite name set; the dense blocks x locals form raised the Symfony build's footprint peak by 757 MB"
created: 2026-09-27
sources:
  - path: src/codegen/callable_reachability.rs
    blob: 662014fdc4987ca3e5f3e86f57253d0b14a52156
    lines: 71-130
    snip: 07f07ab6a3a7
    anchor: "struct LocalFacts {"
---

# Callable reachability keeps sparse block states: absent means unknown, and the dense form cost 757 MB for one method

## Fact

CallableReachabilityAnalysis used to hold a Vec<CallableNameSet> over EVERY local for EVERY block,
twice (block_inputs and block_outputs). On Symfony's ReflectionAttribute::newInstance (43k EIR
instructions) that was 757 MB, allocated inside FunctionContext::new before any code was emitted;
on macOS the freed memory stays in the footprint, so one method raised the whole build's peak.

The block state is now LocalFacts: a map of only the locals holding a FINITE name set; a local
absent from it is unknown (the entry state, and the lattice top). The join keeps a local only
when both sides know it, with the union of names, and drops it past MAX_FINITE_CALLABLE_NAMES.
It was cross-checked against the dense analysis on every Symfony body (24 821 analyses, 0
mismatches, 114 839 finite values).

The trap for a later edit: in a sparse join, "absent" means UNKNOWN, never "no names". The test
one_sided_store_leaves_the_join_open covers it, and it only has teeth because the store reaches
the join FIRST (through a separate `then` block); with the open side arriving first the join
never sees a known-vs-absent pair.

## Why

A later edit that reads an absent local as an empty set would silently narrow callable candidates; one that goes back to dense state brings the memory back
