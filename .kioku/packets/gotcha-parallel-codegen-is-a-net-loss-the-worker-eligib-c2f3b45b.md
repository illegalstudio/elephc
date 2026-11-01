---
id: gotcha-parallel-codegen-is-a-net-loss-the-worker-eligib-c2f3b45b
type: gotcha
title: "Parallel codegen is a net loss: the worker-eligible bodies are the cheap ones"
description: "Back-to-back jobs=8 vs jobs=1 on Symfony --web, and why no tuning saves the parallel pass"
created: 2026-09-20
sources:
  - path: src/codegen/block_emit.rs
    blob: e631a67203fac238d654727004a133be4d23cacf
    lines: 374-400
    snip: a6dbcd51408f
    anchor: "fn codegen_jobs() -> usize {"
---

# Parallel codegen is a net loss: the worker-eligible bodies are the cheap ones

## Fact

A codegen worker cannot emit a body that reaches a shared cache, and the bodies that reach one
are exactly the EXPENSIVE ones — anything with a callable, a descriptor or a dispatch table. On
the Symfony `--web` module 7 396 of 8 786 bodies are worker-eligible, and they are all small.

Back to back, same tree, same machine:

| | jobs=8 | jobs=1 |
|---|---|---|
| Generating native code | 214.41 s | **130.42 s** |
| whole build, wall | 470 s | **315 s** |
| whole build, user CPU | 512.29 s | **457.63 s** |
| emitted assembly | 43 359 381 lines | **43 206 352 lines** |

The decisive number is not the total, it is this: the serial pass emits ALL 8 786 bodies in
93.48 s, while under `jobs=8` the same pass takes 98.35 s for the 1 390 DEFERRED bodies alone.
The 7 396 bodies the workers produce cost the serial pass approximately nothing. So the parallel
pass spends its whole wall time, and eight threads of CPU, on the cheapest 5% of the work — and
emits 153 029 lines MORE, because two workers that reach the same host-scoped helper each keep a
copy the merge cannot recognise.

`codegen_jobs()` now returns 1 unless `ELEPHC_CODEGEN_JOBS` says otherwise. The machinery stays,
because the shape that would make it pay is known: populate the shared caches BEFORE the workers
start, so no body defers. A getter hit would then have to stop touching the cache, which is only
sound for entries emitted before the worker pass — their text is in the pre-body region and is
never discarded.

Measuring this needs care: on this machine wall AND CPU are both inflated about 2x by load
(the same build read 224.60 s CPU at load 21 and 512.29 s at load 38), so only back-to-back
runs compare. `ELEPHC_CODEGEN_SHARED_TOUCHES=1` prints the numbers above.

## Why

The parallel pass looks like free speed and costs 50% more wall time and 12% more CPU; the default is now 1.
