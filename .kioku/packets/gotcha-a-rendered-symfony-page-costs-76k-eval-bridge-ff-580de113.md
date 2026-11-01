---
id: gotcha-a-rendered-symfony-page-costs-76k-eval-bridge-ff-580de113
type: gotcha
title: "A rendered Symfony page costs 76k eval-bridge FFI entries, and 48k of them only re-describe the native surface"
description: "Measured on the first --web build that actually renders: php -S serves the same page in 2ms, elephc in 42ms, and the trace says the cost is native-surface REGISTRATION repeated per eval site"
created: 2026-09-17
verified_by: "ELEPHC_EVAL_TRACE=all on one 200 response: 81231 lines, 76505 phase=ffi_entry; curl warm 42ms vs 2ms for php -S on a byte-identical page"
sources:
  - path: src/codegen/lower_inst/builtins/eval/method_registration.rs
    blob: 1877682e6331336f447b138fd2a9d8c47f918faf
superseded_by: "gotcha-the-76k-ffi-entries-per-page-figure-is-dead-star-d1ae79d5"
superseded_why: "Startup priming in the --web master now runs the registration helper before the fork; measured 105288 entries before any request and ~2900 per request, so the 48k-per-page registration replay no longer happens."
---

# A rendered Symfony page costs 76k eval-bridge FFI entries, and 48k of them only re-describe the native surface

## Fact

Measured 2026-09-17 on the first `--web` build that serves the Twig page (byte-identical to
`php -S`):

    php -S   warm:  2.0 ms
    elephc   warm: 42.0 ms

ONE request traces 81 231 eval lines, 76 505 of them `phase=ffi_entry`. The top symbols are not
render work at all:

     8790  __elephc_eval_register_native_method_bridge_support
     8790  __elephc_eval_register_native_method
     8675  __elephc_eval_register_native_method_return_type
     6710  __elephc_eval_register_native_method_param_flags
     6710  __elephc_eval_register_native_method_param
     6629  __elephc_eval_register_native_method_param_type
     3876  __elephc_eval_register_native_property_type
     2322  __elephc_eval_register_native_property_default_scalar
     2273  __elephc_eval_scope_get
     2229  __elephc_eval_scope_set

About 48 000 of the 76 500 crossings only TELL THE INTERPRETER WHAT THE COMPILED CLASSES ARE.
`register_eval_native_method` and its siblings are emitted INLINE AT EACH EVAL SITE, so every
interpreted include re-registers the surface, and Twig does many per render.

The lever is to register the immutable native surface ONCE per process — before the prefork, so
children inherit it — instead of once per eval site. That is an architectural change to how the
eval context is built, not a local fix, which is why it is recorded rather than attempted here.

Two things that are NOT the problem, so they are not re-investigated: the garbage collector (the
`__rt_gc_safepoint` work already took c1 p50 from 14.3 ms to 7.64 ms on the 404 path), and
preloading more files (7 -> 133 files moved the bridge count 381 -> 387).

Wall-clock on this machine is unusable while syspolicyd/Kaspersky/XprotectService scan the 220 MB
binaries, and the shipped binary is STRIPPED so `sample(1)` shows only `???`. The trace COUNT is
the load-independent signal; use `--keep-symbols` if a real profile is needed.

## Why

It is the single biggest lever on --web latency and it is setup work, not render work
