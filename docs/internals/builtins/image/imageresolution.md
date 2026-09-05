---
title: "imageresolution() — internals"
description: "Compiler internals for imageresolution(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 523
---

## `imageresolution()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/image_prelude.rs`:2312](https://github.com/illegalstudio/elephc/blob/main/src/image_prelude.rs#L2312) (`imageresolution`)
- **Function symbol**: `imageresolution()`


### Lowering notes

- Implemented by the compiler-injected image prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function imageresolution(mixed $image, ?int $resolution_x = null, ?int $resolution_y = null): mixed
```

## What the type checker enforces

- **Arity**: takes 1–3 arguments (2 optional).

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `imageresolution()`](../../../php/builtins/image/imageresolution.md)
