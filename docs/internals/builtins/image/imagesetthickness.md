---
title: "imagesetthickness() — internals"
description: "Compiler internals for imagesetthickness(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 529
---

## `imagesetthickness()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/image_prelude.rs`:2635](https://github.com/illegalstudio/elephc/blob/main/src/image_prelude.rs#L2635) (`imagesetthickness`)
- **Function symbol**: `imagesetthickness()`


### Lowering notes

- Implemented by the compiler-injected image prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function imagesetthickness(mixed $image, int $thickness): bool
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `imagesetthickness()`](../../../php/builtins/image/imagesetthickness.md)
