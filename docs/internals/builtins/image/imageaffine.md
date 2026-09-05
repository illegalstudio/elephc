---
title: "imageaffine() — internals"
description: "Compiler internals for imageaffine(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 455
---

## `imageaffine()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/image_prelude.rs`:3210](https://github.com/illegalstudio/elephc/blob/main/src/image_prelude.rs#L3210) (`imageaffine`)
- **Function symbol**: `imageaffine()`


### Lowering notes

- Implemented by the compiler-injected image prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function imageaffine(mixed $image, array $affine, ?array $clip = null): mixed
```

## What the type checker enforces

- **Arity**: takes 2–3 arguments (1 optional).

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `imageaffine()`](../../../php/builtins/image/imageaffine.md)
