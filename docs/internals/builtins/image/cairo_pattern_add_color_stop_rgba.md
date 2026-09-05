---
title: "cairo_pattern_add_color_stop_rgba() — internals"
description: "Compiler internals for cairo_pattern_add_color_stop_rgba(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 423
---

## `cairo_pattern_add_color_stop_rgba()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/image_prelude.rs`:13810](https://github.com/illegalstudio/elephc/blob/main/src/image_prelude.rs#L13810) (`cairo_pattern_add_color_stop_rgba`)
- **Function symbol**: `cairo_pattern_add_color_stop_rgba()`


### Lowering notes

- Implemented by the compiler-injected image prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function cairo_pattern_add_color_stop_rgba(mixed $pattern, float $offset, float $red, float $green, float $blue, float $alpha): void
```

## What the type checker enforces

- **Arity**: takes exactly 6 arguments.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `cairo_pattern_add_color_stop_rgba()`](../../../php/builtins/image/cairo_pattern_add_color_stop_rgba.md)
