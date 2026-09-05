---
title: "imagebmp() — internals"
description: "Compiler internals for imagebmp(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 460
---

## `imagebmp()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/image_prelude.rs`:3619](https://github.com/illegalstudio/elephc/blob/main/src/image_prelude.rs#L3619) (`imagebmp`)
- **Function symbol**: `imagebmp()`


### Lowering notes

- Implemented by the compiler-injected image prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function imagebmp(mixed $image, ?string $file = null, bool $compressed = true): bool
```

## What the type checker enforces

- **Arity**: takes 1–3 arguments (2 optional).

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `imagebmp()`](../../../php/builtins/image/imagebmp.md)
