---
title: "cairo_curve_to() — internals"
description: "Compiler internals for cairo_curve_to(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 402
---

## `cairo_curve_to()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/image_prelude.rs`:13502](https://github.com/illegalstudio/elephc/blob/main/src/image_prelude.rs#L13502) (`cairo_curve_to`)
- **Function symbol**: `cairo_curve_to()`


### Lowering notes

- Implemented by the compiler-injected image prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function cairo_curve_to(mixed $context, float $x1, float $y1, float $x2, float $y2, float $x3, float $y3): void
```

## What the type checker enforces

- **Arity**: takes exactly 7 arguments.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `cairo_curve_to()`](../../../php/builtins/image/cairo_curve_to.md)
