---
title: "imagecrop() — internals"
description: "Compiler internals for imagecrop(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 494
---

## `imagecrop()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/image_prelude.rs`:3124](https://github.com/illegalstudio/elephc/blob/main/src/image_prelude.rs#L3124) (`imagecrop`)
- **Function symbol**: `imagecrop()`


### Lowering notes

- Implemented by the compiler-injected image prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function imagecrop(mixed $image, mixed $rect = ['x' => 0, 'y' => 0, 'width' => 0, 'height' => 0]): mixed
```

## What the type checker enforces

- **Arity**: takes 1–2 arguments (1 optional).

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `imagecrop()`](../../../php/builtins/image/imagecrop.md)
