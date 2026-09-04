---
title: "imagecropauto() — internals"
description: "Compiler internals for imagecropauto(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 495
---

## `imagecropauto()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/image_prelude.rs`:3149](https://github.com/illegalstudio/elephc/blob/main/src/image_prelude.rs#L3149) (`imagecropauto`)
- **Function symbol**: `imagecropauto()`


### Lowering notes

- Implemented by the compiler-injected image prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function imagecropauto(mixed $image, int $mode = IMG_CROP_DEFAULT, float $threshold = 0.5, int $color = -1): mixed
```

## What the type checker enforces

- **Arity**: takes 1–4 arguments (3 optional).

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `imagecropauto()`](../../../php/builtins/image/imagecropauto.md)
