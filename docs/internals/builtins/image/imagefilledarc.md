---
title: "imagefilledarc() — internals"
description: "Compiler internals for imagefilledarc(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 500
---

## `imagefilledarc()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/image_prelude.rs`:2771](https://github.com/illegalstudio/elephc/blob/main/src/image_prelude.rs#L2771) (`imagefilledarc`)
- **Function symbol**: `imagefilledarc()`


### Lowering notes

- Implemented by the compiler-injected image prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function imagefilledarc(mixed $image, int $center_x, int $center_y, int $width, int $height, int $start_angle, int $end_angle, int $color, int $style): bool
```

## What the type checker enforces

- **Arity**: takes exactly 9 arguments.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `imagefilledarc()`](../../../php/builtins/image/imagefilledarc.md)
