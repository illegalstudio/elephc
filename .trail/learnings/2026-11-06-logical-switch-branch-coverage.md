---
title: Logical leaves are branch points; a switch is one branch point with cases+1 edges
author: OpenCode
date: 2026-11-06
branch: detached/db62f09226
cost: { minutes: 180 }
touches:
  - src/ir/function.rs
  - src/ir/mod.rs
  - src/ir_lower/expr/branch_conditions.rs
  - src/ir_lower/stmt/switches.rs
  - src/codegen/context.rs
  - src/codegen/lower_term.rs
  - src/debug_metadata.rs
  - src/test_command/coverage.rs
  - tests/codegen/testing_command.rs
  - docs/beyond-php/testing.md
  - docs/compiling/debug-metadata.md
---

## Decided

- `BranchSite` is now a source construct with a control-flow `BranchSiteKind`:
  - `Conditional { block }`: two edges on that block's `CondBr`;
  - `StaticSwitch { block }`: one edge per `Switch` case label plus default;
  - `DynamicSwitch { case_blocks, default_block }`: one match edge per case-check
    `CondBr` plus the final default branch.
- `lower_condition_tree` records every **leaf** of an `&&`/`||`/`!` tree, not just
  a simple condition. `if ($a > 0 && $b > 0)` therefore has two branch points, each
  with its own true/false edges. The leaf's edges are the operand's own truth
  values; `!` can invert which outer arm they reach, but the coverage counts are
  still the operand's outcomes.
- A `switch` is one branch point with `case labels + 1` edges. Static integer
  dispatch guards each match edge and the final default edge; the miss edges only
  continue the compare chain. Dynamic string/mixed dispatch guards each case
  check's then edge and the final default `Br`. A case after an earlier match is
  never checked and reports all its edges unreached.
- `FunctionContext.branch_site_ids` caches the allocated site ids per branch site
  and emits the `@branch` marker exactly once. This is needed because a dynamic
  switch's default edge may be lowered before its case checks in block order.
- `@branch` markers now carry `kind=conditional|switch` and `edges=<id,...>`.
  Conditional markers keep `then_site`/`else_site` as a compatibility spelling.
- `.edbg.json` branch entries carry `kind` and `edges`; conditional entries also
  keep `then_site`/`else_site`. Coverage prefers `edges` and falls back to the two
  legacy fields, so old sidecars still load.
- The coverage model is `BranchCoverage { source_id, line, edges: Vec<u64> }`.
  Text/LCOV/Cobertura/HTML all derive totals by summing `edges.len()`, so a switch
  contributes three or more edges instead of being forced into a pair.
- LCOV emits one `BRDA:<line>,<branch>,<edge>,<taken>` per edge. If a branch point
  was never reached, every edge is `-`; a reached switch branch with unhit cases
  reports `0` for those edges. `BRF`/`BRH` count all switch edges.
- Cobertura keeps one `<condition>` per branch point; its `coverage` is the
  percentage of that branch point's edges. A switch line's `condition-coverage`
  aggregates all branch points on the line.
- HTML keeps `T`/`E` badges for two-edge conditionals and uses `E0`…`En` for a
  multi-edge switch.

## Tried and ruled out

- Treating each `&&`/`||` as one branch point: the compiler lowers the leaves to
  separate `CondBr`s, and one point would hide which operand short-circuited.
  Per-leaf points are the only source-level mapping.
- Treating each dynamic switch case check as a two-edge branch point: the else edge
  is "continue to the next case", not a source decision, and a three-case switch
  would report 6 edges instead of 4. The N+1 edge model matches the source construct.
- Guarding case bodies instead of the dispatch match: fallthrough enters a body
  from the previous case, so a body guard would attribute the previous case's match
  to the wrong label.
- Counting the default as an extra two-edge conditional: it is one edge of the
  switch, not a separate condition.
- Rewriting the old conditional metadata fields in place: keeping
  `then_site`/`else_site` for conditional entries avoids breaking existing clients
  and tests; `edges` is the general form.
- Emitting `-` for every edge of a partially reached switch: unhit cases are real
  zero-count edges, not unreachable ones.

## Watch out

- A static switch with a multi-label case (`case 1: case 2:`) gets one edge per
  label, both targeting the same body. That is intentional: each label is a source
  decision.
- A `switch` default written in the middle of the source keeps the dispatch edge
  last in `edges`; edge order is case labels in source order, then default.
- `!` reports the operand's edges, not the negated expression's. A client that
  displays "taken" as "the if body ran" must account for polarity; the counts are
  condition outcomes.
- The `@branch` marker for a switch has no `then_site`/`else_site`. The metadata
  parser requires `edges` for that case.
- `BranchSite` is no longer `Copy`; codegen clones it before allocating sites.
- `--debug` disables EIR optimization, so `branch_sites` and the block ids they
  name survive. If branch coverage is ever enabled for optimized builds, every
  function-cloning/block-rewriting pass must remap or drop branch sites.
- The logical/switch fixture uses `strlen("abc")`/`strlen("")` and `substr` so the
  conditions and switch subject stay runtime values under `--debug`.
