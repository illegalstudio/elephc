---
title: "imagecolorat() — internals"
description: "Compiler internals for imagecolorat(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 465
---

## `imagecolorat()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/image_prelude.rs`:2335](https://github.com/illegalstudio/elephc/blob/main/src/image_prelude.rs#L2335) (`imagecolorat`)
- **Function symbol**: `imagecolorat()`


### Lowering notes

- Implemented by the compiler-injected image prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function imagecolorat(mixed $image, int $x, int $y): int
```

## What the type checker enforces

- **Arity**: takes exactly 3 arguments.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `imagecolorat()`](../../../php/builtins/image/imagecolorat.md)
