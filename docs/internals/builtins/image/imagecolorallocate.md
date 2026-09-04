---
title: "imagecolorallocate() — internals"
description: "Compiler internals for imagecolorallocate(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 463
---

## `imagecolorallocate()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/image_prelude.rs`:2223](https://github.com/illegalstudio/elephc/blob/main/src/image_prelude.rs#L2223) (`imagecolorallocate`)
- **Function symbol**: `imagecolorallocate()`


### Lowering notes

- Implemented by the compiler-injected image prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function imagecolorallocate(mixed $image, int $red, int $green, int $blue): int
```

## What the type checker enforces

- **Arity**: takes exactly 4 arguments.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `imagecolorallocate()`](../../../php/builtins/image/imagecolorallocate.md)
