---
title: "imagewebp() — internals"
description: "Compiler internals for imagewebp(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 536
---

## `imagewebp()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/image_prelude.rs`:3633](https://github.com/illegalstudio/elephc/blob/main/src/image_prelude.rs#L3633) (`imagewebp`)
- **Function symbol**: `imagewebp()`


### Lowering notes

- Implemented by the compiler-injected image prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function imagewebp(mixed $image, string $file = null, int $quality = -1): bool
```

## What the type checker enforces

- **Arity**: takes 1–3 arguments (2 optional).

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `imagewebp()`](../../../php/builtins/image/imagewebp.md)
