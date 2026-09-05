---
title: "cairo_pattern_create_radial() — internals"
description: "Compiler internals for cairo_pattern_create_radial(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 425
---

## `cairo_pattern_create_radial()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/image_prelude.rs`:13779](https://github.com/illegalstudio/elephc/blob/main/src/image_prelude.rs#L13779) (`cairo_pattern_create_radial`)
- **Function symbol**: `cairo_pattern_create_radial()`


### Lowering notes

- Implemented by the compiler-injected image prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function cairo_pattern_create_radial(float $cx0, float $cy0, float $radius0, float $cx1, float $cy1, float $radius1): mixed
```

## What the type checker enforces

- **Arity**: takes exactly 6 arguments.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `cairo_pattern_create_radial()`](../../../php/builtins/image/cairo_pattern_create_radial.md)
