---
title: "imagefilledellipse() — internals"
description: "Compiler internals for imagefilledellipse(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 501
---

## `imagefilledellipse()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/image_prelude.rs`:2733](https://github.com/illegalstudio/elephc/blob/main/src/image_prelude.rs#L2733) (`imagefilledellipse`)
- **Function symbol**: `imagefilledellipse()`


### Lowering notes

- Implemented by the compiler-injected image prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function imagefilledellipse(mixed $image, int $center_x, int $center_y, int $width, int $height, int $color): bool
```

## What the type checker enforces

- **Arity**: takes exactly 6 arguments.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `imagefilledellipse()`](../../../php/builtins/image/imagefilledellipse.md)
