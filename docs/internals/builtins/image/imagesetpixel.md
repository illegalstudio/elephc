---
title: "imagesetpixel() — internals"
description: "Compiler internals for imagesetpixel(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 528
---

## `imagesetpixel()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/image_prelude.rs`:2252](https://github.com/illegalstudio/elephc/blob/main/src/image_prelude.rs#L2252) (`imagesetpixel`)
- **Function symbol**: `imagesetpixel()`


### Lowering notes

- Implemented by the compiler-injected image prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function imagesetpixel(mixed $image, int $x, int $y, int $color): bool
```

## What the type checker enforces

- **Arity**: takes exactly 4 arguments.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `imagesetpixel()`](../../../php/builtins/image/imagesetpixel.md)
