---
title: "imagecolormatch() — internals"
description: "Compiler internals for imagecolormatch(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 472
---

## `imagecolormatch()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/image_prelude.rs`:2563](https://github.com/illegalstudio/elephc/blob/main/src/image_prelude.rs#L2563) (`imagecolormatch`)
- **Function symbol**: `imagecolormatch()`


### Lowering notes

- Implemented by the compiler-injected image prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function imagecolormatch(mixed $image1, mixed $image2): bool
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `imagecolormatch()`](../../../php/builtins/image/imagecolormatch.md)
