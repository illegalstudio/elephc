---
title: "cairo_matrix_init_translate() — internals"
description: "Compiler internals for cairo_matrix_init_translate(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 415
---

## `cairo_matrix_init_translate()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/image_prelude.rs`:13836](https://github.com/illegalstudio/elephc/blob/main/src/image_prelude.rs#L13836) (`cairo_matrix_init_translate`)
- **Function symbol**: `cairo_matrix_init_translate()`


### Lowering notes

- Implemented by the compiler-injected image prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function cairo_matrix_init_translate(float $tx, float $ty): mixed
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `cairo_matrix_init_translate()`](../../../php/builtins/image/cairo_matrix_init_translate.md)
