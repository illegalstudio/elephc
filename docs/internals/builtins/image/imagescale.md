---
title: "imagescale() — internals"
description: "Compiler internals for imagescale(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 526
---

## `imagescale()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/image_prelude.rs`:3101](https://github.com/illegalstudio/elephc/blob/main/src/image_prelude.rs#L3101) (`imagescale`)
- **Function symbol**: `imagescale()`


### Lowering notes

- Implemented by the compiler-injected image prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function imagescale(mixed $image, int $width, int $height = -1, int $mode = IMG_BILINEAR_FIXED): mixed
```

## What the type checker enforces

- **Arity**: takes 2–4 arguments (2 optional).

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `imagescale()`](../../../php/builtins/image/imagescale.md)
