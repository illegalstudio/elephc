---
title: "cairo_image_surface_get_height() — internals"
description: "Compiler internals for cairo_image_surface_get_height(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 409
---

## `cairo_image_surface_get_height()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/image_prelude.rs`:13331](https://github.com/illegalstudio/elephc/blob/main/src/image_prelude.rs#L13331) (`cairo_image_surface_get_height`)
- **Function symbol**: `cairo_image_surface_get_height()`


### Lowering notes

- Implemented by the compiler-injected image prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function cairo_image_surface_get_height(mixed $surface): int
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `cairo_image_surface_get_height()`](../../../php/builtins/image/cairo_image_surface_get_height.md)
