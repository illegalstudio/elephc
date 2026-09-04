---
title: "cairo_pattern_add_color_stop_rgb() — internals"
description: "Compiler internals for cairo_pattern_add_color_stop_rgb(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 422
---

## `cairo_pattern_add_color_stop_rgb()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/image_prelude.rs`:13795](https://github.com/illegalstudio/elephc/blob/main/src/image_prelude.rs#L13795) (`cairo_pattern_add_color_stop_rgb`)
- **Function symbol**: `cairo_pattern_add_color_stop_rgb()`


### Lowering notes

- Implemented by the compiler-injected image prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function cairo_pattern_add_color_stop_rgb(mixed $pattern, float $offset, float $red, float $green, float $blue): void
```

## What the type checker enforces

- **Arity**: takes exactly 5 arguments.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `cairo_pattern_add_color_stop_rgb()`](../../../php/builtins/image/cairo_pattern_add_color_stop_rgb.md)
