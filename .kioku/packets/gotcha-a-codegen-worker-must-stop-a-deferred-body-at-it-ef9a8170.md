---
id: gotcha-a-codegen-worker-must-stop-a-deferred-body-at-it-ef9a8170
type: gotcha
title: "A codegen worker must stop a deferred body at its first cache touch, and must NOT roll the cache back"
description: "Two bounded experiments on the same loop: one took the worker pass from 40s to 14s, the other from 40s to 121s"
created: 2026-09-20
sources:
  - path: src/codegen/block_emit.rs
    blob: 069f11595b18a1e36f1823ee6486f4c55c35c800
    lines: 490-535
    snip: 95c0476ad138
    anchor: "let start = emitter.checkpoint();"
---

# A codegen worker must stop a deferred body at its first cache touch, and must NOT roll the cache back

## Fact

A worker cannot emit a body that reaches a shared codegen cache, and it discovers that only by
emitting. Two changes to that loop, measured on the Symfony --web module:

**Stop the body at the first touch — keep this.** The check is one Option read per instruction
(`SharedCodegenState::must_defer`), and the abort is a `CodegenIrError` carrying a `deferred`
flag so it survives `at()`. Measured:

    worker_pass  40.06s -> 13.74s
    discarded    2 101 439 143 -> 962 699 386 bytes
    .s           byte-identical (1 605 475 628), all eight Symfony routes still identical

**Also roll the shared cache back — never do this.** It looks bounded once the abort exists, and
it is not: one instruction's lowering can emit thousands of descriptors, so the abort cannot land
INSIDE it, and every following body then re-emits the helpers the rollback removed. Measured:

    worker_pass  40.06s -> 120.60s
    discarded    2 101 439 143 -> 29 397 554 561 bytes  (deferred_share 99.0%)

Not rolling the cache back is sound because of an invariant worth stating: a body that touches a
cache is ALWAYS deferred, so a KEPT body never references a cached helper. A cache entry naming
discarded text can therefore only be read by another body that is itself discarded, or by the
serial pass, whose text is kept. Any new cache family inherits that invariant for free — and
breaks it the moment a getter stops touching.

Read the numbers with `ELEPHC_CODEGEN_SHARED_TOUCHES=1`.

## Why

Both look like the same idea and one of them is ruinous; the numbers are the only way to tell them apart.
