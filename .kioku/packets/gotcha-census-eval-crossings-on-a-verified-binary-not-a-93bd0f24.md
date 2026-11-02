---
id: gotcha-census-eval-crossings-on-a-verified-binary-not-a-93bd0f24
type: gotcha
title: "Census eval crossings on a verified binary, not a clobbered one"
description: "The delta-trace method that names every compiler gap as a per-request count, with the corrected figures: 3681 crossings on Symfony's simplest route, 11 interpreted includes, 6 class declarations. Supersedes an earlier packet whose numbers were 3x too high because the binary had been clobbered and lacked the compiled DI container"
created: 2026-09-21
verified_by: "evaldelta.sh on a 174 MB binary carrying 1191 container symbols, N=1 vs N=5, priming subtracted"
sources:
  - path: crates/elephc-magician/src/eval_trace.rs
    blob: c9dc8e30f8104b74a70b94f65831005b034cf4be
    lines: 1-70
    snip: af90c61be51e
    anchor: "use std::sync::atomic::{AtomicU8, Ordering};"
supersedes: "gotcha-census-the-eval-crossings-not-the-profile-to-fin-dd75fae6"
---

# Census eval crossings on a verified binary, not a clobbered one

## Fact

To find what still escapes compilation, CENSUS THE EVAL CROSSINGS rather than profile. A profile
says where time goes; this says why the code is interpreted at all, as a named list of compiler
gaps with counts. `scratchpad/evaldelta.sh` does it.

THE METHOD: a trace over a server's lifetime counts one-time priming as if it recurred (the
bridge registers tens of thousands of lines before and during the first request). Run N=1 and
N=5 and take (five - one) / 4 per phase. That cancels priming AND first-request warmup in one
subtraction and needs no knowledge of where a request begins in the log.

**CHECK THE BINARY BEFORE TRUSTING ANY CENSUS.** This supersedes an earlier packet whose numbers
were three times too high because they came from a binary that had been clobbered by another
build and was missing the compiled DI container. A half-compiled binary answers every route
byte-identically and simply runs more of the app in the interpreter, so the census reflects the
BUILD, not the compiler. Confirm `strings <binary> | grep -c ContainerXvDiqOo` is non-zero and the
binary is in the expected size range (174-263 MB for this app) first.

MEASURED on Symfony `--web`, route `/plain`, on a binary that carries the container:

    TOTAL                     3681 crossings per request
    include_ok                  11        class_decl_ok               6
    input_cached                31
    quiet_property_fetch_push  902        quiet_property_fetch_pop  902
    scope_get                  270        scope_set                 225
    object_is_a                194        context_new                62

The two `quiet_property_fetch` counters are 49% of the total on their own and are a thread-local
depth counter, so they are cheap per call but enormous in count. `scope_get`/`scope_set` are
variable reads and writes from interpreted PHP: the BRIDGE is not the cost, the existence of
interpreted code that uses it is. Optimising the bridge is the wrong target; removing the
interpreted code is the right one.

WHAT REMAINS INTERPRETED, and why: 31 sources are not in the compiler's catalog at ALL, so no
include guard can help them — Composer's chain (`autoload.php`, `autoload_real.php`,
`platform_check.php`, `autoload_static.php`, 12 eager files, 5 nested `bootstrap8x.php`), the
entry re-required by Symfony Runtime, and 9 container/routing files. 695 of 700 catalog sources
are already flagged compiler-included.

`set_web_sapi` / `set_strict_php` / `set_php_version_id` read exactly once per dynamic include
(11 each here, equal to `__elephc_eval_include`): they are emitted by `lower_dynamic_include`,
not an independent redundancy. Remove the includes and they go with them.

## Why

A half-compiled binary answers every route byte-identically, so a census reflects the BUILD rather than the compiler unless the binary is checked first.
