---
title: "imagecolorset() — internals"
description: "Compiler internals for imagecolorset(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 475
---

## `imagecolorset()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/image_prelude.rs`:2577](https://github.com/illegalstudio/elephc/blob/main/src/image_prelude.rs#L2577) (`imagecolorset`)
- **Function symbol**: `imagecolorset()`


### Lowering notes

- Implemented by the compiler-injected image prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function imagecolorset(mixed $image, int $color, int $red, int $green, int $blue, int $alpha = 0): bool
```

## What the type checker enforces

- **Arity**: takes 5–6 arguments (1 optional).

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `imagecolorset()`](../../../php/builtins/image/imagecolorset.md)
