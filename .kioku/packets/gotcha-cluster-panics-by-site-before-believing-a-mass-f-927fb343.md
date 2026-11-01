---
id: gotcha-cluster-panics-by-site-before-believing-a-mass-f-927fb343
type: gotcha
title: "Cluster panics by site before believing a mass failure count"
description: "A 607-failure codegen run was read as the tree's standing state; 183 of them were one assertion at one site, from a concurrent agent landing a builtin contract entry ahead of its eval registry binding. Clustering panic sites overturned the conclusion in three commands. Also records that codegen_tests has no baseline at all, only --lib does"
created: 2026-09-21
verified_by: "grep of the 100MB run log: 183 identical messages at one panic site, and the source now carries the binding"
sources:
  - path: crates/elephc-magician/src/interpreter/builtins/registry/mod.rs
    blob: 29d28a373a328ca9d8839c01b600bdf40f0f1cbd
    lines: 82-95
    snip: 51a0bb1c1b8f
    anchor: "fn validate_shared_eval_coverage(by_name: &HashMap<String, usize>) {"
---

# Cluster panics by site before believing a mass failure count

## Fact

Before treating a mass failure count as "the tree's standing state", CLUSTER THE PANICS BY SITE.
One site with hundreds of hits is one transient defect, not a baseline.

Worked example, and it overturned a subagent's conclusion in three commands. A 99-minute
`cargo test --release --test codegen_tests` reported `9586 passed; 607 failed`, and the agent
concluded the 607 was pre-existing — reasonably, since it had A/B'd its own change on one module
and found identical failures on both builds. But:

    grep -aoE "panicked at [^:]*:[0-9]+:[0-9]+:" codegen-run.log \
      | sed -E 's/:[0-9]+:[0-9]+:$//' | sort | uniq -c | sort -rn

    260  tests/codegen/support/runner.rs
    183  crates/elephc-magician/src/interpreter/builtins/registry/mod.rs   <- ONE SITE
     61  tests/codegen/eval.rs
     ...

and every one of those 183 carried the SAME message:

    shared builtin contract substr_compare requires an eval registry binding

`validate_shared_eval_coverage` asserts that every shared builtin contract has its declared
Magician route. A concurrent agent had landed the contract entry in
`crates/elephc-builtin-contract/src/catalog_data.rs` ahead of the eval registry binding in
`crates/elephc-magician/src/interpreter/builtins/`, so for a window EVERY test that touches the
eval builtin registry aborted. The number was contaminated; the tree was repaired later and the
count was never retaken.

TWO RULES THIS GIVES:

1. A failure count from a shared, concurrently-edited worktree is a measurement of the tree AT
   THAT MOMENT, not of the change under test. Cluster by panic site before drawing any
   conclusion from it, and say which clusters you attributed.
2. Adding a shared builtin contract entry and its eval registry binding in separate edits breaks
   EVERY other build in the worktree while the two are apart — CLI and --web alike, and the
   failure names it in a magician file, far from the edit. Land them together. The sibling
   assertion in `src/builtins/registry.rs` does the same for the AOT registry binding, with the
   message "requires an AOT registry binding".

STILL OPEN AS A CONSEQUENCE: there is NO baseline for `codegen_tests` (~10 300 tests, ~99 min).
`scratchpad/baseline-failures.txt` covers only `--lib` (29 standing failures). Until a clean
codegen baseline is taken on a settled tree, nobody can tell a regression in that suite from the
standing state, and the honest thing is to say so rather than quote a number.

## Why

A contract entry and its registry binding landed apart break EVERY build in a shared worktree, and the failure names a magician file far from the edit, so the next person misattributes it.
