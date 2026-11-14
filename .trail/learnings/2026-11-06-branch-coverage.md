---
title: Branch coverage reuses site counters but must not reuse source-site stop semantics
author: OpenCode
date: 2026-11-06
branch: detached/db62f09226
cost: { minutes: 150 }
touches:
  - src/ir/function.rs
  - src/ir/builder.rs
  - src/ir_lower/expr/branch_conditions.rs
  - src/codegen/lower_term.rs
  - src/codegen/block_emit.rs
  - src/debug_metadata.rs
  - crates/elephc-debug/src/lib.rs
  - src/test_command/coverage.rs
  - tests/codegen/cli.rs
  - tests/codegen/testing_command.rs
---

## Decided

- Branch points are EIR data: `Function.branch_sites` stores the block and source
  span of each **simple** source-level condition. `lower_condition_tree` records
  the site on the leaf that emits the user `CondBr`, so a condition whose
  expression creates helper blocks still points at the block that actually
  branches. `&&`/`||`/`!` are excluded because they lower to several internal
  edges; `switch` is excluded until it has a source-level mapping.
- Codegen allocates two site ids per branch point, emits an `@branch` marker
  (`source`, `line`, `col`, `then_site`, `else_site`) and two edge guards. The
  guards call the existing preserving site wrapper, which counts the edge. The
  branch ids share the global site counter and flag table but are **not** in the
  metadata `sites` table.
- The debug runtime now ignores sites absent from the metadata `sites` map before
  applying pause/step/breakpoint logic. They are still counted, but a branch
  guard must not consume a `next` step or overwrite the frame's last known line.
  This was a real regression: `next` from an `if` line silently ran to exit
  because the branch guard consumed the step.
- `@branch` markers become a `branches` array in `.edbg.json`:
  `{source_id, line, column, then_site, else_site}`. `test --coverage` joins the
  two edge counters; hits on a branch site never affect line coverage.
- Text reports add `Branches: covered/total` and per-source `branches:` lines.
  LCOV emits `BRDA:<line>,<branch>,<edge>,<taken>` plus `BRF`/`BRH`; Cobertura
  emits `branch="true" condition-coverage="…"` and one `<condition>` per branch.
  An edge that never executed is `-` in LCOV; an executed-but-untaken edge is
  `0`.
- `BRF`/`BRH` count **edges** (2 per branch point), not branch points, matching
  LCOV's per-`BRDA` record model.

## Tried and ruled out

- Instrumenting every `CondBr` by looking at the block's last source line: it
  counts compiler guards, short-circuit internals, null checks and type probes as
  source branches. The EIR side table is the source-level truth.
- Recording branch sites in `lower_condition` at the entry block: a condition
  like `if (foo($a && $b))` creates helper blocks before the final `CondBr`, so
  the entry block can differ from the block that branches. Recording at the leaf
  `CondBr` is exact.
- Making branch edges stop in the debugger: they have no source site; stopping
  would show a branch id and break stepping. The runtime early-returns for
  unknown sites after counting.
- Emitting branch ids into `sites`: line coverage would count branch edges as
  executed lines and the metadata site count would no longer match guards.

## Watch out

- Logical `&&`/`||`/`!` conditions and `switch` have no branch records yet; their
  lines still get line coverage.
- A branch point is attributed to the condition expression's span. A multi-line
  condition uses its start line; a ternary uses its condition line.
- Branch sites are allocated after line sites in a function, so the site id
  ordering is not purely source order. Coverage joins by id, never by order.
- The debugger's `ELEPHC_DEBUG_SITES=1` dump lists branch ids alongside line
  sites; tests that validate the dump must read the `branches` table too.
- `--debug` disables EIR optimization, so branch sites survive; if branch
  coverage is ever enabled for optimized builds, every function-cloning pass must
  remap or drop `branch_sites`.
- The e2e fixture uses `strlen("abc")` so the condition stays runtime under
  `--debug`: two `if`s produce `BRF:4`/`BRH:2` (first then edge, second else
  edge), and Cobertura shows `branch-rate="0.5000"`.
