---
title: "cairo_set_source_rgb() — internals"
description: "Compiler internals for cairo_set_source_rgb(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 439
---

## `cairo_set_source_rgb()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/image_prelude.rs`:13387](https://github.com/illegalstudio/elephc/blob/main/src/image_prelude.rs#L13387) (`cairo_set_source_rgb`)
- **Function symbol**: `cairo_set_source_rgb()`


### Lowering notes

- Implemented by the compiler-injected image prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function cairo_set_source_rgb(mixed $context, float $red, float $green, float $blue): void
```

## What the type checker enforces

- **Arity**: takes exactly 4 arguments.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `cairo_set_source_rgb()`](../../../php/builtins/image/cairo_set_source_rgb.md)
