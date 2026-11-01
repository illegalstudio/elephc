---
id: gotcha-the-76k-ffi-entries-per-page-figure-is-dead-star-d1ae79d5
type: gotcha
title: "The 76k-FFI-entries-per-page figure is dead: startup priming already hoisted them out of the request"
description: "Measured on the current build: 105288 bridge entries before any request, ~2900 per request; the per-request cost is no longer the eval bridge"
created: 2026-09-20
sources:
  - path: src/codegen/frame.rs
    blob: 4fdb5bffd1d87656cb1a00156821ba5649f77a06
    lines: 735-770
    snip: f2187e0a7189
    anchor: "fn emit_eval_metadata_priming(ctx: &mut FunctionContext<'_>) {"
  - path: crates/elephc-magician/src/context/global_registry.rs
    blob: 0086f9783d2d4b1e9d063edb3979352b33ef89de
    lines: 196-258
    snip: 9a883e309ac1
    anchor: "pub(crate) fn publish_global_eval_aot_metadata(context: &ElephcEvalContext) {"
supersedes: "gotcha-a-rendered-symfony-page-costs-76k-eval-bridge-ff-580de113"
---

# The 76k-FFI-entries-per-page figure is dead: startup priming already hoisted them out of the request

## Fact

`emit_eval_metadata_priming` (src/codegen/frame.rs) allocates ONE eval context in the `--web`
master and runs the module-wide registration helper before `elephc_web_run` forks, so the
process-global snapshot exists in the master's heap and every forked worker and every
request-handler child inherits it copy-on-write. Each later context's first
`__elephc_eval_context_try_sync_aot_metadata` adopts ~17 `Arc`s and returns 1, which makes
`ensure_eval_context` branch straight past the registration helper.

Census on the current build (`ELEPHC_EVAL_TRACE=ffi`, count lines by symbol -- no counter needs
writing, the runtime already announces every `__elephc_eval_*` entry):

    entries before any request      105 288      <- the priming, once, in the master
    request 1 /plain                  2 921
    request 2 /plain                  2 954
    request 1 /  (template route)     4 201
    request 2 /                       4 234

So the old "76 000 entries per page, 48 000 of them registration" is gone. What is left per
request, in order: `quiet_property_fetch_push`/`_pop` (55% of the entries, but each is only a
thread-local counter bump -- roughly 24 us in total, NOT a lever), `scope_get`/`scope_set`,
`object_is_a`, and ~100 `context_new`/`try_sync`/`context_free`.

**The bridge is no longer where a request's time goes.** Measured with `abbench.py` (raw
sockets, interleaved A/B, minimum is the headline) at load 18:

    /plain         elephc min 6.36 ms   php -S 1.11 ms
    /greet/world   elephc min 6.43 ms   php -S 1.14 ms
    /  (template)  elephc min 8.89 ms   php -S 1.32 ms   (p50 11.96 ms)

Every route pays the SAME ~6.4 ms floor, including one that returns 33 bytes; the template
route adds only ~2.5 ms on top. So the target is the fixed per-request cost, not the route.
`sample` on the worker shows the stack is compiled native code throughout -- but the binary is
stripped to 275 symbols by default, so build with `--keep-symbols` or every frame prints as a
raw address.

## Why

Three advisor models independently proposed rebuilding a mechanism that already exists and works; the stale number is what sent them there.
