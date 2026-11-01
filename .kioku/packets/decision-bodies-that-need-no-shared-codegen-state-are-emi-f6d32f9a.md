---
id: decision-bodies-that-need-no-shared-codegen-state-are-emi-f6d32f9a
type: decision
title: "Bodies that need no shared codegen state are emitted in parallel"
description: "Discovered by emitting and rolling back rather than by a predicate; deterministic, opt-out with ELEPHC_CODEGEN_JOBS=1"
created: 2026-09-20
sources:
  - path: src/codegen/block_emit.rs
    blob: 415654f395442399d367c4761f36ad100e95ad44
    lines: 120-260
    snip: a4a390e4bfc9
    anchor: "let mut inventory = BackendInventory::from_env();"
  - path: src/codegen_support/data_section.rs
    blob: 89b6ca7d881ffdd70e368700432795e6d1ed6684
    lines: 100-200
    snip: 77fe1cabf235
    anchor: "comm_entries: usize,"
---

# Bodies that need no shared codegen state are emitted in parallel

## Fact

Every body is emitted on a worker that owns its emitter, its data shard and its own EMPTY
`SharedCodegenState`. A body whose emission touches a shared cache -- read or write -- is rolled
back and its index recorded; those are re-emitted serially, in module order, against the real
state. A read counts as much as a write: the worker's cache is empty, so a read misses and the
body would emit a private copy of a helper the module must hold exactly once.

Discovering the split by TRYING, rather than by a predicate over the EIR ops, is the point. A
predicate would have to be kept in step with every lowering that reaches a cache, and getting it
wrong is a duplicate or missing symbol. Rolling back cannot be wrong.

Deterministic without locking: which bodies fall back is a property of the module, workers get
fixed contiguous chunks, and both groups keep module order. Three runs at 8 jobs produce
byte-identical assembly (`scratchpad/det-runs.sh`). `ELEPHC_CODEGEN_JOBS=1` takes the old single
pass; `--counters`, `--instrument`, `ELEPHC_CODEGEN_SURVEY` and `ELEPHC_BACKEND_INVENTORY` all
stay serial because each depends on emission order.

Three things this needed, each a bug when missing:

* Label ids scoped to the body (see the label decision packet). BLOCK labels were the one place
  still drawing from the module-wide counter, and with per-worker state three prelude helpers
  whose names cap to `__elephc_array_merge_gra` each defined
  `L_eir___elephc_array_merge_gra_foreach_exit_11`. 15 array tests caught it.
* The data section rolled back WITH the text. A callable descriptor is a data entry that points
  at a text label the body defines, so keeping the data and dropping the text left
  `.quad L_eir_..._callable_instance_method_...` with nothing to resolve. 6 eval tests caught it.
* The reflection materializers counted as a cache too -- they live on `SharedReflectionState`,
  not on `SharedCodegenState`.

Verified: `codegen_tests arrays` 914 passed/30 failed, exactly the serial baseline; determinism
tests 3/3; all eight Symfony routes byte-identical to `php -S` from a `--web` build with the
parallel pass on.

The ceiling is 12.4% of code generation, because the deferred bodies carry 87.6% of the assembly
-- see the dispatch-ladder packet for why, and for what to do instead.

## Why

Code generation is 55% of a Symfony --web build and was single-threaded
