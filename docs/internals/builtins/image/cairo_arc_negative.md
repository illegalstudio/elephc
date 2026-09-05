---
title: "cairo_arc_negative() — internals"
description: "Compiler internals for cairo_arc_negative(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 399
---

## `cairo_arc_negative()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/image_prelude.rs`:13550](https://github.com/illegalstudio/elephc/blob/main/src/image_prelude.rs#L13550) (`cairo_arc_negative`)
- **Function symbol**: `cairo_arc_negative()`


### Lowering notes

- Implemented by the compiler-injected image prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function cairo_arc_negative(mixed $context, float $xc, float $yc, float $radius, float $angle1, float $angle2): void
```

## What the type checker enforces

- **Arity**: takes exactly 6 arguments.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `cairo_arc_negative()`](../../../php/builtins/image/cairo_arc_negative.md)
