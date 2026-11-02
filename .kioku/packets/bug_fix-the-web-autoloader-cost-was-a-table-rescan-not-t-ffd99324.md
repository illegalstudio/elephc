---
id: bug_fix-the-web-autoloader-cost-was-a-table-rescan-not-t-ffd99324
type: bug_fix
title: "The --web autoloader cost was a table rescan, not the 31 includes"
description: "Hoisting per-request includes into the master before fork is NOT the win: 31 includes but only 5 class declarations and no function redeclarations per request. The cost was a per-declaration scan of the compile-time reflection method table; memoizing it took 2.0 points of worker CPU. Also fixes the instrument rule: a latency A/B cannot resolve under 5-10% on this machine, only a matched-pair profile can"
created: 2026-09-21
verified_by: "Matched-pair 20s sample of the serving worker on one binary with a single env lever; 600-request soak with per-reply sha256"
sources:
  - path: crates/elephc-magician/src/interpreter/runtime_ops.rs
    blob: 80323a4143f0b7b4fcfadb26887060acf92e82f4
    lines: 21-60
    snip: 1467416377fd
    anchor: "pub enum AotMemberNameKind {"
  - path: crates/elephc-magician/src/runtime_hooks/ops/reflection.rs
    blob: 0ab3c0586a15fce3033a022ff672b5850ebef50d
    lines: 11-437
    snip: 31168008fc68
    anchor: "macro_rules! impl_reflection_ops {"
  - path: src/codegen_support/runtime/data/user.rs
    blob: b6d3ce38902ea2ee0eca2d94879a332c604f7f9a
    lines: 1380-1517
    snip: b3cf44741b63
    anchor: "fn emit_eval_reflection_method_lookup_data("
---

# The --web autoloader cost was a table rescan, not the 31 includes

## Fact

The Symfony `--web` autoloader subtree looked like "31 files re-executed per request", ~26% of
the request. It is not that. Measured with an include census: **31 includes but only 5 class
declarations and no function redeclarations** per request — the polyfill bootstraps are already
skipped by their own `function_exists` guards, `autoload_static.php` is 253 lines and
`App_KernelProdContainer.php` is 22. The file contents were never the cost.

So HOISTING THE PER-REQUEST INCLUDES INTO THE MASTER BEFORE FORK IS NOT THE WIN, and the
hypothesis that "a class declaration from a source the compiler knows is a program constant,
replayed per request" — mine — was the wrong shape. The cost inside that subtree was a TABLE
SCAN re-run per declaration, not the declaration.

WHAT THE COST ACTUALLY WAS: `__elephc_eval_reflection_method_names` walked
`_eval_reflection_methods` — `.quad` literals emitted at compile time
(`src/codegen_support/runtime/data/user.rs:1503`) with a fixed `_eval_reflection_method_count`,
three emitted readers, and **no writer anywhere** in `src/` or the magician FFI. Two of its three
readers were already memoized off that same table. Memoizing the third is exactly as sound.

Matched pair, ONE binary, only the env lever differing, 20s `sample` of the serving worker on
`/plain` — the only instrument that resolves an effect this size here:

    …reflection_method_names self       324 (2.18%)  ->   47 (0.30%)
    …same, inclusive                    388 (2.62%)  ->   98 (0.62%)
    _rt_strcasecmp                      689 (4.64%)  ->  382 (2.42%)

~2.0 percentage points of worker CPU. Autoloader subtree self-time 14.1% -> 8.1%.

THE INSTRUMENT RULE THIS ESTABLISHES: run-to-run spread on `/plain` min was **6.76-11.87 ms at
loads 13-36**. A 2% change cannot be resolved against that, so on this machine a latency A/B is
NOT a valid instrument for anything under roughly 5-10%; a matched-pair profile on one binary
with a single env lever is. Do not accept a latency delta as evidence for a small change, and do
not dismiss a profile-attributed win because latency did not visibly move.

Beware the lever's blast radius: `ELEPHC_AOT_REFLECTION_MEMO=0` disables the WHOLE `aot_memo`
family, not one table, so an A/B on it measures the family.

RANKED LEADS STILL OPEN, re-profiled after the above (the ranking changed, so re-profile again
before picking one):

1. `_rt_strcasecmp` 382 (2.42%) and `__rt_instanceof_lookup` 380 (2.40%) — now the top two, both
   name-based lookup. `__rt_instanceof_lookup` linearly scans ~2 396 entries; the cure already
   exists next door as `__rt_callable_lookup_composite_hash` (case-insensitive FNV-1a plus open
   addressing). Riskier than a memo: it is a codegen rewrite.
2. `ElephcEvalContext::class_is_a` — the single biggest self-time item left inside the autoloader
   subtree (109 samples). It allocates a `Vec<String>` of parents AND of interfaces plus a
   normalized `String` per element on EVERY call. This is an allocation fix, not a memo: its
   inputs are request state.
3. ~0.47% of remaining name scans. Cheap to extend, ONE TRAP: `reflection/class_lookup.rs` uses
   an extractor that does NOT strip a leading `\` while `builtins/class_metadata.rs` does, so the
   two families must not be merged blindly; `class_api.rs:548` hands its handle straight to PHP
   and must keep the handle path.

Pre-existing and unattributed: the worker leaks ~146 KB/request (87 MB over 600), identical with
the memo off. A 600-request soak showed 0 drifted response bodies.

## Why

My own hypothesis -- a class declaration from a source the compiler knows is a program constant replayed per request -- was the wrong shape, and an include census refuted it before any code was written. Recording it so nobody spends the rebuild cycle retrying it.
