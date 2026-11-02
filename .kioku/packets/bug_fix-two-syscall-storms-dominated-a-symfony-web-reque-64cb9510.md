---
id: bug_fix-two-syscall-storms-dominated-a-symfony-web-reque-64cb9510
type: bug_fix
title: "Two syscall storms dominated a Symfony --web request: realpath per include, getenv per FFI entry"
description: "What the profile said, what each cost, and the measured result of caching both"
tags: [magician, performance, symfony, web]
created: 2026-09-16
verified_by: "sample(1) on the 192-file worker before/after; 404 body byte-identical to php -S at every step"
sources:
  - path: crates/elephc-magician/src/realpath_cache.rs
    blob: 7ee6bf2a6beb0bcae55302c0c2fbe6bee03af430
  - path: crates/elephc-magician/src/eval_trace.rs
    blob: c9dc8e30f8104b74a70b94f65831005b034cf4be
  - path: crates/elephc-magician/src/interpreter/include_exec.rs
    blob: 6a98cdc4323ad44441fc1076cec525437291fd56
---

# Two syscall storms dominated a Symfony --web request: realpath per include, getenv per FFI entry

## Fact

Profiling the 192-file Symfony worker found two costs that had nothing to do with PHP semantics.

1. `eval_include_key()` called `std::fs::canonicalize()` on EVERY interpreted include. `realpath()`
   issues one `getattrlist` per path COMPONENT on macOS, the generated container fragments sit ten
   components deep, and a `--web` worker re-includes the same tree every request:
   `__getattrlist` = 1901 samples, the largest cost after the idle `kevent`.
   Fixed by `realpath_cache`, php-src's own answer (`realpath_cache_ttl` 120 s,
   `realpath_cache_size` 4096 entries). Result: **1901 -> 35 samples**.

2. `ELEPHC_EVAL_TRACE` was read from the ENVIRONMENT at 109 sites, including
   `trace_eval_ffi_entry`, which runs on every eval FFI entry. On macOS `getenv` takes
   `os_unfair_lock` and scans `environ` linearly: `getenv` = 1095 samples, for a facility that was
   switched OFF. Fixed by `eval_trace`, an `AtomicU8` verdict.
   Result: `trace_eval_ffi_entry` **361 -> 0**, `getenv` **1095 -> 291** (the rest is other callers).

The trace verdict is INVALIDATED rather than a `OnceLock`: PHP's `putenv()` can set the variable
mid-run, so `putenv` calls `eval_trace::invalidate()` and the observable behaviour stays what
reading the environment every time gave.

Measured on the same machine under the same load, elephc vs `php -S` on the same 404:
ratio **5.9x -> 4.0x** (p50 9.70 ms -> 8.48 ms while `php -S` moved 1.64 -> 2.12 ms with the load).
Read the RATIO, not the absolute: the wall clock on this box is unusable while the codegen suite
runs, which is why both fixes were judged on the sample instead.

Next in the profile, in order: `_rt_heap_free`/`_xzm_free`/`_rt_heap_alloc` (allocation churn),
`__elephc_eval_reflection_method_flags` 221, `_rt_strcasecmp` 206, `_rt_file_get_contents` 199
plus `stat` 101 — files are still being READ every request despite the parse cache, worth a look.
