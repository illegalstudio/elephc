---
title: Path coverage is a grouping over terminal branch edges, not a second counter family
author: OpenCode
date: 2026-11-06
branch: detached/db62f09226
cost: { minutes: 170 }
touches:
  - src/ir/function.rs
  - src/ir/mod.rs
  - src/ir/builder.rs
  - src/ir_lower/expr/branch_conditions.rs
  - src/ir_lower/stmt/switches.rs
  - src/codegen/block_emit.rs
  - src/codegen/lower_term.rs
  - src/debug_metadata.rs
  - src/test_command/coverage.rs
  - tests/codegen/testing_command.rs
  - docs/beyond-php/testing.md
  - docs/compiling/debug-metadata.md
---

## Decided

- A **path** is one terminal short-circuit route through a logical condition or
  one dispatch route through a switch. For `a && b` the routes are `a` false,
  `a` true and `b` true, and `a` true and `b` false. `a || b` is symmetric. A
  switch gets one route per case label plus default.
- Paths reuse the terminal edge's branch guard/counter. `PathDescriptor` carries
  `branch_site` (index into `Function::branch_sites`), `edge` and a label. No
  second BSS counter family or runtime hook is introduced.
- EIR gets `Function::path_sites`. Logical path sites are planned after the
  leaves are lowered: `terminal_paths` consumes leaves in source order, keeps the
  left-false routes of `And` / left-true routes of `Or`, and appends the right
  subtree's routes. `Not` inverts the route truth but keeps its terminal edge.
- Switch path sites are recorded by both static and dynamic dispatch lowering,
  one descriptor per case label plus default, referencing the switch branch site.
- Codegen emits `@path` markers **after all blocks**: `emit_path_markers` asks the
  existing `branch_sites` cache for the terminal edge's site id and emits
  `@path source=… line=… col=… kind=logical|switch site=… label=…`. This also
  allocates a branch site whose block was never emitted; such a path then reports
  zero hits.
- Metadata groups consecutive `@path` markers into a `paths` table:
  `{function_id, source_id, line, column, kind, paths:[{site,label}]}`.
- The coverage report gains `paths`. Counts reuse `counts[site]`; text adds
  `Paths: x/y` and per-source `paths`/`uncovered paths`; LCOV adds
  `PATH:<line>,<index>,<hits>`, `PTF:<total>` and `PTH:<covered>`; Cobertura adds
  `path-rate`, `paths-covered`/`paths-valid`, `path-coverage="x% (a/b)"` and
  `<condition type="path">`; HTML adds a per-file Path coverage table with
  hit/miss route badges.
- Branch coverage is unchanged: the same terminal edge still counts as its
  branch edge, and the path table is a separate grouping over those site ids.

## Tried and ruled out

- Full Cartesian prefix expansion for nested logical expressions: `(a || b) && c`
  has prefixes that share a terminal `c` edge. Without a per-path state variable,
  two prefixes reaching the same terminal edge cannot be distinguished from one
  counter. The report groups them into the terminal route instead of claiming a
  coverage value it cannot measure. This is documented as terminal short-circuit
  path coverage, not MC/DC.
- A second path counter family: it would duplicate the branch guards, double the
  BSS footprint and add a runtime state machine. Reusing terminal edge guards is
  exact for the terminal-route model.
- Recording paths at every leaf edge: left-true/false and right-true/false are
  branch edges, not terminal routes; the path count would equal the branch edge
  count and add no information.
- Deriving paths in the coverage renderer from `branches`: the logical grouping
  is not in the branch table, and `(a || b) && c` cannot be reconstructed from
  per-leaf aggregates. EIR records the grouping and terminal edges.
- Emitting `@path` markers at each condition lowering: site ids are allocated
  by codegen when guards are emitted, and a later block can still allocate a
  branch site. End-of-function emission is the only point where every referenced
  edge has a final id.

## Watch out

- `@path` markers are emitted after the machine peephole rewrite; they are
  comments and do not affect instruction layout, but they must stay before
  `@endfn` (they do).
- A path label is the terminal condition's overall truth and source position,
  e.g. `false@7:16`. It does not spell the whole prefix; that is intentional for
  grouped terminal routes.
- `Not` inverts the route truth but not the terminal edge. `!(a && b)` labels
  `a=false` as true and `b=true` as false even though the leaf edge is unchanged.
- Switch labels are index-based (`case0`, `case1`, `default`), not source case
  values, because the switch terminator only carries integer case values. A
  multi-label case contributes one route per label.
- Path groups share branch edge counters with branch coverage. Do not add their
  hits to branch totals; the tests assert branch totals stay unchanged.
- `paths` is a v1 metadata extension. Consumers that only read `branches` keep
  working; LCOV `PATH`/`PTF`/`PTH` are elephc-specific records.
- The e2e fixture has `&&`, `||` and a 3-route switch; it asserts 3/9 paths,
  `BRF:11`/`BRH:4`, `PATH` indices, Cobertura path attrs/conditions, the HTML
  path table, and the metadata group kinds/labels.

## Superseded

The terminal-route grouping described above was replaced on the same day by full
reachable prefix combinations: a hidden 2-bit-per-leaf state word plus one dedicated
counter per route. See `.trail/learnings/2026-11-06-full-prefix-path-coverage.md`.
