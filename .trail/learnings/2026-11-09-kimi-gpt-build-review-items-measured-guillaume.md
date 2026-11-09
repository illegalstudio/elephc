---
title: Kimi/GPT build review items measured on the Symfony Demo
author: Guillaume
date: 2026-11-09
branch: feat/closed-world-symfony-build-speed
cost: { minutes: 112, tokens: 328607 }
touches:
  - src/codegen/block_emit.rs
  - Cargo.lock
  - Cargo.toml
  - src/codegen_support/emit.rs
  - src/linker/inhouse/mod.rs
  - src/optimize/reachability/graph.rs
  - src/optimize/reachability/tests.rs
  - src/codegen/runtime_callable_invoker.rs
  - tests/codegen/regressions/invoker_name_tables.rs
  - src/optimize/reachability/reconcile.rs
---

## Decided
- Measured every Kimi/GPT build review item on the Symfony Demo default console build (one core, `.elephc-work/pad/demoexp.sh`): 3381 G -> 3241 G instructions in one day, parity 25/25 + 249/249 throughout.
- Kept: incremental vtable-family walk (16.1 s -> small; `ELEPHC_REACH_VERIFY`, always on in tests), reconcile grouping reachable methods by class once (10 s -> 0.2 s, was quadratic), XXH3 for content-shared body names (was SipHash), in-tree assembler pre-sizing (neutral).
- Instruments: `ELEPHC_PRUNE_STATS`, `ELEPHC_DECL_REACH_TIMES` (now splits reconcile), `ELEPHC_OP_STATS` OPSITE lines for sites >= 4000 instructions, `ELEPHC_ASM_TRACE` (render vs in-house per slice).
## Tried and ruled out
- `ELEPHC_OUTLINE_OPS`: +3.4% compile, -1% size, web link fails in parallel codegen (bead elephc-k0f8).
- Invoker memo by shape: 79 474 invokers lowered for 8 956 bodies, yet only -0.65% compile, and a same-key/different-body case (named-argument scan) -> reverted. Emitted-then-rolled-back text is a poor proxy for compile time.
- Prune before typecheck: the Demo prunes NOTHING (dynamic_method hazard keeps every method), so it cannot shrink checking.
- Positional dynamic new through the arg-container table: -39 MB, ~-1.2% compile, but 6 semantic tests diverge (bead elephc-znoo).
## Watch out
- The assembler phase is ~110 s CPU: parsing 2.7 GB of slice text, not encoding (bead elephc-psew). Dynamic-property ladders are ~4.2 M op instructions in a few whole-program helpers (bead elephc-xqop).
- Two Demo builds differ by a few bytes of Symfony cache constants: binary cmp across builds is not a determinism check.
- Stats flags (`ELEPHC_OP_STATS`) slow the build; compare instruction counts only between runs with the same flags.
