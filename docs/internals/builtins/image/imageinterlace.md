---
title: "imageinterlace() — internals"
description: "Compiler internals for imageinterlace(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 512
---

## `imageinterlace()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/image_prelude.rs`:3344](https://github.com/illegalstudio/elephc/blob/main/src/image_prelude.rs#L3344) (`imageinterlace`)
- **Function symbol**: `imageinterlace()`


### Lowering notes

- Implemented by the compiler-injected image prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function imageinterlace(mixed $image, bool $enable = null): int
```

## What the type checker enforces

- **Arity**: takes 1–2 arguments (1 optional).

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `imageinterlace()`](../../../php/builtins/image/imageinterlace.md)
