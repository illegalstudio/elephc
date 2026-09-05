---
title: "imagefilledrectangle() — internals"
description: "Compiler internals for imagefilledrectangle(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 503
---

## `imagefilledrectangle()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/image_prelude.rs`:2699](https://github.com/illegalstudio/elephc/blob/main/src/image_prelude.rs#L2699) (`imagefilledrectangle`)
- **Function symbol**: `imagefilledrectangle()`


### Lowering notes

- Implemented by the compiler-injected image prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function imagefilledrectangle(mixed $image, int $x1, int $y1, int $x2, int $y2, int $color): bool
```

## What the type checker enforces

- **Arity**: takes exactly 6 arguments.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `imagefilledrectangle()`](../../../php/builtins/image/imagefilledrectangle.md)
