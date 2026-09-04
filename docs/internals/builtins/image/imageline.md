---
title: "imageline() — internals"
description: "Compiler internals for imageline(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 516
---

## `imageline()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/image_prelude.rs`:2648](https://github.com/illegalstudio/elephc/blob/main/src/image_prelude.rs#L2648) (`imageline`)
- **Function symbol**: `imageline()`


### Lowering notes

- Implemented by the compiler-injected image prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function imageline(mixed $image, int $x1, int $y1, int $x2, int $y2, int $color): bool
```

## What the type checker enforces

- **Arity**: takes exactly 6 arguments.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `imageline()`](../../../php/builtins/image/imageline.md)
