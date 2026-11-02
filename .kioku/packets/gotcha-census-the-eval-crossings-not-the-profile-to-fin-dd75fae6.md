---
id: gotcha-census-the-eval-crossings-not-the-profile-to-fin-dd75fae6
type: gotcha
title: "Census the eval crossings, not the profile, to find what escapes compilation"
description: "A delta trace between 1 and 5 requests cancels priming and names every compiler gap as a count: 11374 eval crossings per request on Symfony's simplest route, 8466 of them FFI entries, 67% of which are variable reads and writes from interpreted PHP. 131 eval contexts per request re-state three program constants 71 times each"
created: 2026-09-21
verified_by: "evaldelta.sh on public/index, route /plain, N=1 vs N=5, priming subtracted"
sources:
  - path: crates/elephc-magician/src/eval_trace.rs
    blob: c9dc8e30f8104b74a70b94f65831005b034cf4be
    lines: 1-70
    snip: af90c61be51e
    anchor: "use std::sync::atomic::{AtomicU8, Ordering};"
superseded_by: "gotcha-census-eval-crossings-on-a-verified-binary-not-a-93bd0f24"
superseded_why: "Its numbers came from a binary clobbered by a concurrent build and missing the compiled DI container: 3681 crossings, 11 includes and 6 class declarations, not 11374, 71 and 55"
---

# Census the eval crossings, not the profile, to find what escapes compilation

## Fact

To answer "what still escapes compilation", do not profile — CENSUS THE EVAL CROSSINGS. A profile
tells you where time goes; this tells you WHY the code is there at all, as a named list of
compiler gaps. `scratchpad/evaldelta.sh` does it.

THE METHOD, and it matters: a trace over a server's lifetime counts one-time priming as if it
recurred (the bridge registers ~78 000 lines before and during the first request). Run N=1 and
N=5 and take (five - one) / 4 per phase. That subtraction cancels priming AND first-request
warmup in one step, and needs no knowledge of where a request begins in the log.

MEASURED on Symfony `--web`, route `/plain` — the SIMPLEST route, 33 bytes of response:

    ffi_entry                      8466        method_dispatch                 421
    native_method_return_contract   270        dynamic_property_lookup/get/set 674
    dynamic_method_start/end        450        object_is_a                     201
    array_reference_read            156        spl_autoload_*                  167
    include_ok                       71        class_decl_ok                    55
    TOTAL                         11374 crossings per request

The FFI entries, which are 74% of that, break down as:

    __elephc_eval_scope_get                     1768
    __elephc_eval_scope_set                     1724
    __elephc_eval_quiet_property_fetch_push     1073
    __elephc_eval_quiet_property_fetch_pop      1073
    __elephc_eval_scope_mark_global_alias        568
    __elephc_eval_context_push/pop_class_scope   235 each
    __elephc_eval_context_new / _free            132 / 121
    __elephc_eval_context_try_sync_aot_metadata  131
    __elephc_eval_set_web_sapi / _strict_php / _php_version_id   71 each

THE THREE CONCLUSIONS:

1. The top four are 67% of the FFI traffic, and they are variable reads and writes
   (`scope_get`/`scope_set`, 3492 per request). The BRIDGE is not the cost; the existence of
   interpreted PHP that uses it 3492 times per request is. Optimising the bridge is the wrong
   target — removing the interpreted code is the right one.
2. 131 EVAL CONTEXTS ARE CREATED PER REQUEST, each syncing AOT metadata, and `set_web_sapi`,
   `set_strict_php` and `set_php_version_id` are re-applied 71 times each. Those three are
   PROGRAM CONSTANTS. 71 is exactly the per-request include count, so each include builds a
   context and re-states them. That is pure redundancy and is separable from the bigger fix.
3. Everything converges on 71 includes + 55 class declarations per request — Composer's eager
   `autoload.files` entries and the container factory. "Maximise compiled code, avoid eval" and
   "get Composer's eager files into the closed world" are the same problem.

CORRECTS AN EARLIER REPORT: a subagent measured "~2 900 FFI entries per request" and concluded
the bridge was not the cost. The conclusion holds; the magnitude does not — it is 8466, and the
number matters because it is what a per-crossing cost gets multiplied by.

## Why

A profile says where time goes; this says why the code is interpreted at all. It also shows the bridge is not the target -- the interpreted PHP that crosses it 3492 times per request is.
