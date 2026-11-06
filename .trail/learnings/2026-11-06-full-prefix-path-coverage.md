---
title: Full logical path coverage needs a hidden state word and one counter per terminal prefix
author: OpenCode
date: 2026-11-06
branch: detached/db62f09226
cost: { minutes: 210 }
touches:
  - src/ir/function.rs
  - src/ir/mod.rs
  - src/ir/builder.rs
  - src/ir_lower/mod.rs
  - src/ir_lower/program.rs
  - src/ir_lower/expr/branch_conditions.rs
  - src/ir_lower/stmt/switches.rs
  - src/codegen/context.rs
  - src/codegen/lower_term.rs
  - src/codegen/block_emit.rs
  - src/debug_metadata.rs
  - src/test_command/coverage.rs
  - tests/codegen/testing_command.rs
  - docs/beyond-php/testing.md
  - docs/compiling/debug-metadata.md
---

## Decided

- A logical path is now one **full reachable short-circuit prefix combination**, not
  one terminal leaf edge. `(a || b) && (c || d)` produces seven routes: `a` or `b`
  can short-circuit the left operand, and each passing left prefix combines with
  `c` true, `d` true, or `d` false.
- Each logical condition gets a hidden `i64` state slot with two bits per leaf:
  `1 << (2 * leaf)` marks "evaluated", `1 << (2 * leaf + 1)` marks "true". The leaf's
  then/else state block ORs in those bits after the ordinary leaf guard and before
  branching to the real target.
- Path planning is a preorder traversal over the AST logical tree. It enumerates
  left paths, keeps short-circuiting ones, and combines passing ones with every
  right path. The plan is capped at 31 leaves (62 state bits) and 128 routes.
- A path's `mask` is the OR of its visited leaf bits. At the terminal state block,
  `Terminator::Br` compares the live state word against each path mask and calls
  that path's dedicated counter on equality before taking the edge. Two prefixes
  ending on the same leaf edge therefore count separately.
- Logical path groups own one dedicated debug site per route. Switch path groups
  reuse their existing branch-site edge counters; `PathSite::switch_branch_site`
  records the branch site and `state_local` is `None`.
- Codegen emits `@pathgroup source=… kind=… truncated=0|1` followed by
  `@path source=… site=… label=…` markers after all blocks. The metadata parser
  turns them into `paths: [{function_id, source_id, line, column, kind, truncated,
  paths:[{site,label}]}]`.
- Labels spell every evaluated leaf: `#0=T,#2=T@23:28`, `#0=F,#1=F@23:20`, and
  switch labels stay `case0`, `case1`, `default`.
- Path planning is gated on the **debug build** through a thread-local
  `ir_lower::debug_build_enabled()` policy set by `program::lower`. Normal builds
  keep the previous CFG and allocate no hidden state; `--emit-ir` on a normal build
  contains no `condition.path` blocks or `__eir_tmp` state.
- Coverage reads the `truncated` flag. Text adds `truncated paths: <lines>` and the
  HTML path table adds a `truncated` badge; branch totals are untouched.

## Tried and ruled out

- Terminal-edge grouping (the previous slice): it cannot distinguish `a=T,c=T` from
  `a=F,b=T,c=T` when both end on the `c` true edge. That was the missing full-prefix
  behavior.
- A binary or 128-bit state word: two bits per leaf in one `i64` is enough for 31
  leaves, far beyond practical source conditions. Larger conditions are omitted from
  path coverage instead of adding a second word and a more complex dispatch.
- One branch counter per path: logical routes share leaf edges, so branch coverage
  would double-count edges. Path routes get their own counters; branch coverage
  remains the edge-pair model.
- Deriving prefixes in the coverage renderer from `branches`: the traversal order
  and short-circuit grouping are not recoverable from per-leaf aggregate counts.
- Emitting path instrumentation in release builds: it changes the CFG and costs
  integer state updates on every condition. The debug-build gate keeps normal
  programs unchanged.
- Rewriting path labels as source text (`a=T && c=T`): source reconstruction is not
  available without parsing, and leaf indices are stable across formatting. The
  labels are compact and unambiguous.

## Watch out

- A path counter is a debug site id; it is not in the `.edbg.json` `sites` table.
  The runtime site hook ignores unknown ids for stepping, and coverage reads them
  only through `paths`.
- Path counters are allocated during block lowering or at function end, depending
  on which terminal dispatches first. `FunctionContext::path_site_ids` caches them
  so the `@path` markers use the same ids.
- State masks use exact equality. A truncated plan can leave states that no mask
  describes; the dispatch falls through to the branch without crediting a path, and
  the group is marked `truncated`.
- A condition with more than 31 leaves is omitted from `paths` because its state
  cannot fit one `i64`; branch coverage still applies.
- The state slot is initialized at the condition's own evaluation point, so a loop
  condition resets each iteration and counters accumulate across iterations.
- `!` inverts the route truth labels but not the terminal leaf edge or mask bits.
- Switch paths count labels, not bodies: `case 1: case 2:` contributes two routes,
  and fallthrough body statements remain line coverage only.
- `--debug` disables EIR optimization, so path block ids and counters survive to
  codegen. If optimized coverage is ever enabled, every block-rewriting pass must
  remap `PathTerminal.block`.
- The e2e fixture calls the same `check()` twice with `(T,F,T,F)` and `(F,T,T,F)`.
  Both terminate at the `c` true edge but hit different prefix counters:
  `PATH:23,3,1` and `PATH:23,6,1`, with `PTF:10`/`PTH:3` and `BRF:11`/`BRH:5`.
