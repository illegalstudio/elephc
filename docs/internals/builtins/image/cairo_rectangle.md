---
title: "cairo_rectangle() — internals"
description: "Compiler internals for cairo_rectangle(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 428
---

## `cairo_rectangle()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/image_prelude.rs`:13519](https://github.com/illegalstudio/elephc/blob/main/src/image_prelude.rs#L13519) (`cairo_rectangle`)
- **Function symbol**: `cairo_rectangle()`


### Lowering notes

- Implemented by the compiler-injected image prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function cairo_rectangle(mixed $context, float $x, float $y, float $width, float $height): void
```

## What the type checker enforces

- **Arity**: takes exactly 5 arguments.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `cairo_rectangle()`](../../../php/builtins/image/cairo_rectangle.md)
