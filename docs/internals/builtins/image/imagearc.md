---
title: "imagearc() — internals"
description: "Compiler internals for imagearc(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 459
---

## `imagearc()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/image_prelude.rs`:2750](https://github.com/illegalstudio/elephc/blob/main/src/image_prelude.rs#L2750) (`imagearc`)
- **Function symbol**: `imagearc()`


### Lowering notes

- Implemented by the compiler-injected image prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function imagearc(mixed $image, int $center_x, int $center_y, int $width, int $height, int $start_angle, int $end_angle, int $color): bool
```

## What the type checker enforces

- **Arity**: takes exactly 8 arguments.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `imagearc()`](../../../php/builtins/image/imagearc.md)
