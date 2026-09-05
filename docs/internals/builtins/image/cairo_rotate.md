---
title: "cairo_rotate() — internals"
description: "Compiler internals for cairo_rotate(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 430
---

## `cairo_rotate()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/image_prelude.rs`:13680](https://github.com/illegalstudio/elephc/blob/main/src/image_prelude.rs#L13680) (`cairo_rotate`)
- **Function symbol**: `cairo_rotate()`


### Lowering notes

- Implemented by the compiler-injected image prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function cairo_rotate(mixed $context, float $angle): void
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `cairo_rotate()`](../../../php/builtins/image/cairo_rotate.md)
