---
title: "imagestringup() — internals"
description: "Compiler internals for imagestringup(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 531
---

## `imagestringup()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/image_prelude.rs`:2913](https://github.com/illegalstudio/elephc/blob/main/src/image_prelude.rs#L2913) (`imagestringup`)
- **Function symbol**: `imagestringup()`


### Lowering notes

- Implemented by the compiler-injected image prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function imagestringup(mixed $image, int $font, int $x, int $y, string $string, int $color): bool
```

## What the type checker enforces

- **Arity**: takes exactly 6 arguments.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `imagestringup()`](../../../php/builtins/image/imagestringup.md)
