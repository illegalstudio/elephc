---
title: A trivial program lowered 95 functions because the session surface and the Throwable helpers were always kept
author: Guillaume
date: 2026-11-22
branch: bead-nhr6
touches:
  - src/session_prelude.rs
  - src/throwable_string_prelude.rs
  - src/pipeline.rs
  - src/ir_passes/concat_reset.rs
  - src/types/checker/driver/functions.rs
  - src/exports.rs
---

`<?php echo 1;` lowered 95 functions (main: 13), which broke the release_local_slot and integer_destinations tests (their assertions are module-wide) and cost build time and size for every program. Two causes, neither in the callable-bloat gates:
1. `session_prelude::inject_cli_session_surface` injected the whole session surface into every CLI program. Its bodies are pruned when unreached, but every pass that decides what the program uses reads the program BEFORE pruning: `session_decode()` calls `unserialize()`, which makes the SPL and DateTime builtin gates register ~80 classes (a `_class_propinit_N` thunk each), and the cookie code's `max()` made `backend_gap_prelude` declare `__elephc_min_max_mixed` in a FORCED group. Fix: inject the surface only when the user's source names one of its functions/classes (calls, literals, catch/extends), may call it dynamically (`unseen_source_may_call`), or an autoloaded class registers a save handler. Probe: `--emit-ir` and count `^  function ` lines; also `[elephc-prune]` via `ELEPHC_PRUNE_STATS=1`.
2. `pipeline` rooted `throwable_string_prelude::FUNCTIONS` for every program. The helpers contain `new TypeError`, which makes lowering emit `Error::__construct/__toString/getTraceAsString` and, through them, the Throwable machinery. Fix: `helpers_needed` roots them only for `--web`/eval builds, source the compiler cannot see, `set_exception_handler`/`error_get_last`, or user source that spells a throwable type (a class name ending in `exception`/`error`, or `throwable`, in a catch, `new` or `extends`). A program that cannot hold a Throwable cannot render one.
Also: `Op::IOverflows` (the overflow flag of a checked integer add, new in the branch) was missing from the concat_reset inert list, so every checked-counter loop kept a `concat_reset` before its return; and `widen_unrefined_params_of_an_uncalled_function` turned the untyped parameters of an `#[Export]` function into `mixed`, which the scalar C ABI refuses (exports keep the `int` seed now).
Test-shape notes: the IR text now starts with a `sources { ... }` header, and the release_local_slot fixture directory is named after the op the tests search for, so those tests strip the header; the single-store call-site publication (`_php_site`) replaced the five `_php_diagnostic_*` stores, so leaf_runtime_calls asserts the `php_site` word; the mixed-bool boundary conversion is its own `cast ... may_warn` instruction ahead of the call.
Left open (bead elephc-nhr6 successor): counter_keys allocation scaling (lowering emits `ArrayGetMixedKey`, boxed element per read, where main emitted `ArrayGet` for a boxed numeric key; array_access.rs `index_is_mixed`) and the min_max overflow allocations (`$sum += max($i, PHP_INT_MAX + $n)` types `float + mixed` as `mixed`, so the accumulator slot is boxed).
