---
title: "imagerotate() — internals"
description: "Compiler internals for imagerotate(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 524
---

## `imagerotate()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/image_prelude.rs`:3185](https://github.com/illegalstudio/elephc/blob/main/src/image_prelude.rs#L3185) (`imagerotate`)
- **Function symbol**: `imagerotate()`


### Lowering notes

- Implemented by the compiler-injected image prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function imagerotate(mixed $image, float $angle, int $background_color, int $ignore_transparent = 0): mixed
```

## What the type checker enforces

- **Arity**: takes 3–4 arguments (1 optional).

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `imagerotate()`](../../../php/builtins/image/imagerotate.md)
