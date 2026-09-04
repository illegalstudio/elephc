---
title: "cairo_move_to() — internals"
description: "Compiler internals for cairo_move_to(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 418
---

## `cairo_move_to()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/image_prelude.rs`:13476](https://github.com/illegalstudio/elephc/blob/main/src/image_prelude.rs#L13476) (`cairo_move_to`)
- **Function symbol**: `cairo_move_to()`


### Lowering notes

- Implemented by the compiler-injected image prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function cairo_move_to(mixed $context, float $x, float $y): void
```

## What the type checker enforces

- **Arity**: takes exactly 3 arguments.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `cairo_move_to()`](../../../php/builtins/image/cairo_move_to.md)
