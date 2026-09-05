---
title: "cairo_matrix_multiply() — internals"
description: "Compiler internals for cairo_matrix_multiply(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 416
---

## `cairo_matrix_multiply()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/image_prelude.rs`:13873](https://github.com/illegalstudio/elephc/blob/main/src/image_prelude.rs#L13873) (`cairo_matrix_multiply`)
- **Function symbol**: `cairo_matrix_multiply()`


### Lowering notes

- Implemented by the compiler-injected image prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function cairo_matrix_multiply(mixed $m1, mixed $m2): mixed
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `cairo_matrix_multiply()`](../../../php/builtins/image/cairo_matrix_multiply.md)
