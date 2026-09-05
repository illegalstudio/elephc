---
title: "imagecolorexact() — internals"
description: "Compiler internals for imagecolorexact(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 470
---

## `imagecolorexact()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/image_prelude.rs`:2375](https://github.com/illegalstudio/elephc/blob/main/src/image_prelude.rs#L2375) (`imagecolorexact`)
- **Function symbol**: `imagecolorexact()`


### Lowering notes

- Implemented by the compiler-injected image prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function imagecolorexact(mixed $image, int $red, int $green, int $blue): int
```

## What the type checker enforces

- **Arity**: takes exactly 4 arguments.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `imagecolorexact()`](../../../php/builtins/image/imagecolorexact.md)
