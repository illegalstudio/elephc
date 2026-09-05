---
title: "cairo_image_surface_create_from_png() — internals"
description: "Compiler internals for cairo_image_surface_create_from_png(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 408
---

## `cairo_image_surface_create_from_png()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/image_prelude.rs`:13309](https://github.com/illegalstudio/elephc/blob/main/src/image_prelude.rs#L13309) (`cairo_image_surface_create_from_png`)
- **Function symbol**: `cairo_image_surface_create_from_png()`


### Lowering notes

- Implemented by the compiler-injected image prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function cairo_image_surface_create_from_png(string $filename): mixed
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `cairo_image_surface_create_from_png()`](../../../php/builtins/image/cairo_image_surface_create_from_png.md)
