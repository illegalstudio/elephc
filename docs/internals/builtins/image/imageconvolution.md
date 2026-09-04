---
title: "imageconvolution() — internals"
description: "Compiler internals for imageconvolution(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 479
---

## `imageconvolution()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/image_prelude.rs`:3279](https://github.com/illegalstudio/elephc/blob/main/src/image_prelude.rs#L3279) (`imageconvolution`)
- **Function symbol**: `imageconvolution()`


### Lowering notes

- Implemented by the compiler-injected image prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function imageconvolution(mixed $image, mixed $matrix, float $divisor, float $offset): bool
```

## What the type checker enforces

- **Arity**: takes exactly 4 arguments.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `imageconvolution()`](../../../php/builtins/image/imageconvolution.md)
