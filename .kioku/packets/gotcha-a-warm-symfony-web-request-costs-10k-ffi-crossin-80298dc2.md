---
id: gotcha-a-warm-symfony-web-request-costs-10k-ffi-crossin-80298dc2
type: gotcha
title: "A warm Symfony --web request costs 10k FFI crossings because the GENERATED container is interpreted, not because of surface registration"
description: "Measured per-request deltas on a single worker: the 48k native-surface registration is once per WORKER and is not the steady-state cost; a warm request is 14942 trace lines dominated by interpreted scope access"
created: 2026-09-18
verified_by: "400-request warmup over 18 workers: p50 42.04ms. Single worker, request line deltas 81231 / +14942 / +14941 / +14942"
sources:
  - path: examples/symfony-app/public/index_preload_hotpath.php
    blob: a9c3e0690dcce65b660e399c831b599d5f7b2be2
  - path: crates/elephc-magician/src/ffi/context.rs
    blob: 8b2355e90eb7ebe8304a874adffd8ea9a4e4eaf4
---

# A warm Symfony --web request costs 10k FFI crossings because the GENERATED container is interpreted, not because of surface registration

## Fact

CORRECTION. An earlier packet read one COLD request's trace and concluded that
`__elephc_eval_register_native_*` (about 48 000 of 76 500 FFI entries) was the latency lever. It is
not. `__elephc_eval_context_try_sync_aot_metadata` imports a process-global snapshot, so the
surface is registered ONCE PER WORKER, and `--web`'s default `Worker` isolation keeps workers
alive across requests (`crates/elephc-web/src/worker.rs` loops on accept).

MEASURE PER-REQUEST DELTAS, NOT ONE TRACE. With `--workers 1` and `ELEPHC_EVAL_TRACE=all`, count
the trace file's lines after each request:

    req1 81231   req2 +14942   req3 +14941   req4 +14942

and benchmark only after every worker is warm (400 requests for 18 workers), or the sample is 18
cold starts:

    n=30  min=39.50ms  p50=42.04ms  p90=46.01ms  max=48.71ms      (php -S serves it in 2ms)

WHAT A WARM REQUEST ACTUALLY DOES: 10 217 FFI entries, 405 INTERPRETED METHOD CALLS, 153
`__elephc_eval_context_new`, 213 scopes. The top symbols are plain interpreted variable access --
`__elephc_eval_scope_get` 2273, `__elephc_eval_scope_set` 2229,
`__elephc_eval_quiet_property_fetch_push`/`_pop` 1126 each, `__elephc_eval_scope_mark_global_alias`
864.

WHY SO MUCH IS INTERPRETED, and this is the actual lever: the interpreted methods belong to
classes that ARE compiled --

     19  Symfony\Component\HttpKernel\Event\KernelEvent::getRequest
     13  ContainerXvDiqOo\App_KernelProdContainer::hasParameter
     10  Twig\Runtime\EscaperRuntime::escape
     10  Twig\Environment::getRuntime
      9  Twig\ExtensionSet::addExtension

Symfony's GENERATED CONTAINER and Twig's COMPILED TEMPLATES are written into `var/cache/` at run
time, so they are loaded through the interpreted include path. Every service the container
constructs is therefore an interpreter-created object, and every later call on one stays
interpreted even when the class itself was compiled. The cost cascades from those two files.

Both ARE real files on disk at build time, which is what `public/index_preload_hotpath.php`
exploits: it replays `get_included_files()` from a real request and `require_once`s the container
out of `var/cache/prod/ContainerXvDiqOo/`. Growing that preload -- not eliminating registration --
is the lever. See [[symfony-web-build-state]] for its current error count.

## Why

It corrects an earlier conclusion in this repo's memory that named the registration as the lever
