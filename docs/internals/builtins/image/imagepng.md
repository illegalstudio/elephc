---
title: "imagepng() — internals"
description: "Compiler internals for imagepng(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 520
---

## `imagepng()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/image_prelude.rs`:3579](https://github.com/illegalstudio/elephc/blob/main/src/image_prelude.rs#L3579) (`imagepng`)
- **Function symbol**: `imagepng()`


### Lowering notes

- Implemented by the compiler-injected image prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function imagepng(mixed $image, ?string $file = null, int $quality = -1, int $filters = -1): bool
```

## What the type checker enforces

- **Arity**: takes 1–4 arguments (3 optional).

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `imagepng()`](../../../php/builtins/image/imagepng.md)
