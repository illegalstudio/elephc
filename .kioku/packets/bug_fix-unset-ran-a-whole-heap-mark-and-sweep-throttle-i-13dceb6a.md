---
id: bug_fix-unset-ran-a-whole-heap-mark-and-sweep-throttle-i-13dceb6a
type: bug_fix
title: "unset() ran a whole-heap mark and sweep; throttle it like php-src buffers roots"
description: "Why the cycle collector dominated a Symfony request, and how a safepoint counter halved per-request latency"
tags: [gc, performance, symfony]
created: 2026-09-16
verified_by: "macOS sample(1) on the worker under load; wrk before/after; the 16 gc/unset/destruct failures were already failing before the change"
sources:
  - path: src/codegen_support/runtime/arrays/gc_safepoint.rs
    blob: 5ad23a2688ef818ec1b2926c64461af666c3c6be
  - path: src/codegen/lower_inst/core_misc.rs
    blob: c8ffcce636eed689800c1ffde33603e1ad2ce787
  - path: src/codegen/frame.rs
    blob: 4d2340c6f9a7ef4c9b685e0694a27fd9cf4ab451
---

# unset() ran a whole-heap mark and sweep; throttle it like php-src buffers roots

## Fact

`__rt_gc_collect_cycles` is a FOUR-PASS MARK AND SWEEP OVER THE WHOLE HEAP, and `unset()` was its
only caller — via `Op::GcCollect`, emitted from `lower_unset_locals` alone. So every `unset()`
walked the entire heap.

Measured with `sample` on the Symfony `--web` worker under load: 2180 of 6024 main-thread samples
in `_rt_gc_collect_cycles` plus 1437 in `_rt_gc_mark_reachable` — more than a third of the CPU.
Everything else in the profile was ordinary COMPILED Symfony code, which is why counting
eval-bridge crossings had pointed at the wrong thing entirely.

`__rt_gc_safepoint` now counts safe points and only calls the collector at
`GC_SAFEPOINT_THRESHOLD` (10,000), mirroring php-src's possible-root buffer
(`GC_THRESHOLD_DEFAULT`), plus a FORCED collection in `emit_main_epilogue` after locals, statics
and globals are released — where php-src also collects, and before the exit so destructors still
run as PHP callbacks.

Result on `examples/symfony-app`: c1 p50 14.3 ms -> 7.64 ms, 66 -> 126 req/s; c10 on 4 workers
263 -> 411 req/s. Response still byte-identical to `php -S`.

Safe to throttle because PHP's own `gc_collect_cycles()` FOLDS TO THE CONSTANT 0 here
(`src/builtins/system/gc.rs`) and calls no helper — no explicit collection request can be
swallowed.

Worth knowing before touching this again: `test_gc_collect_cycles_reclaims_*` already FAIL on this
branch, so the collector was walking the whole heap hundreds of times per request without
reclaiming those shapes at all.
