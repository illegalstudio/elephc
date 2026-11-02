---
id: gotcha-rolling-a-deferred-body-s-shared-cache-entries-b-f42631ce
type: gotcha
title: "Rolling a deferred body's shared-cache entries back makes the parallel pass quadratic"
description: "It fixes a missing symbol and turns 2.6 GB of discarded worker text into 65 GB; the partial migration has no good configuration"
created: 2026-09-20
sources:
  - path: src/codegen/block_emit.rs
    blob: 42baff1c6941c30437dcbeebdb2b5b6787654ccf
    lines: 370-420
    snip: f50b531e8696
    anchor: "fn codegen_jobs() -> usize {"
  - path: src/codegen/shared_state.rs
    blob: 53af5de02dcfe6e0c5a145ef4167cf312ec864e8
    lines: 300-345
    snip: b2095e7c176e
    anchor: "pub(super) fn unkeyed_cache_touches(&self) -> usize {"
---

# Rolling a deferred body's shared-cache entries back makes the parallel pass quadratic

## Fact

The parallel body pass defers a body that touches a shared cache and re-emits it serially. Two
ways to handle the cache entries that body made, and BOTH are wrong while most bodies defer:

* **Keep them.** A later body reuses a cached label whose defining text was discarded with the
  deferred body. The linker says so:
  `"_eir_shared_callable_invoker_ff5afa6f14b8_0", referenced from: _str_0 in index.slice17.o`.
  This only becomes reachable once helper symbols are key-derived; before that each host minted
  its own name and the stale entry was never reused across bodies in a way that mattered.
* **Roll them back.** Correct — all eight Symfony routes byte-identical — and quadratic. The next
  body in that worker re-emits every helper it needs, defers, and is undone again:

      worker_pass=1388.36s   deferred_bytes=64,974,740,882   deferred_share=99.4%
      Generating native code | 1467.67 s | 93.2%

  against 38 s and 2.6 GB without it.

So a PARTIAL migration to key-derived helper symbols has no good configuration. Both failures
vanish at zero deferral, which is the target: every family reachable from a worker, no rollback
needed because nothing is discarded. Getting there needs the accumulating caches precomputed
before the workers start — see the packet on caches that hold module-wide state.

The verified configuration to return to meanwhile: no family keyed, no cache rollback, deferral
as it was. Measured `worker_pass=38.23s serial_tail=41.06s`, codegen 110.66 s, all eight routes
byte-identical, determinism 3/3. The keyed-helper machinery (`emit_keyed_helper`,
`keyed_global_label`, the `@helper` marker dedup, `SharedCodegenState::checkpoint`) stays in the
tree and inert: it is correct, it is what zero deferral needs, and it costs nothing unused.

## Why

Both halves of the trap were hit in one session; the way out is zero deferral, not a better rollback
