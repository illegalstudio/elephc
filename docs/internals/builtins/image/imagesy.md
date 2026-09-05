---
title: "imagesy() — internals"
description: "Compiler internals for imagesy(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 533
---

## `imagesy()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/image_prelude.rs`:2278](https://github.com/illegalstudio/elephc/blob/main/src/image_prelude.rs#L2278) (`imagesy`)
- **Function symbol**: `imagesy()`


### Lowering notes

- Implemented by the compiler-injected image prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function imagesy(mixed $image): int
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `imagesy()`](../../../php/builtins/image/imagesy.md)
