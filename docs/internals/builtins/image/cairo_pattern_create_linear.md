---
title: "cairo_pattern_create_linear() — internals"
description: "Compiler internals for cairo_pattern_create_linear(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 424
---

## `cairo_pattern_create_linear()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/image_prelude.rs`:13765](https://github.com/illegalstudio/elephc/blob/main/src/image_prelude.rs#L13765) (`cairo_pattern_create_linear`)
- **Function symbol**: `cairo_pattern_create_linear()`


### Lowering notes

- Implemented by the compiler-injected image prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function cairo_pattern_create_linear(float $x0, float $y0, float $x1, float $y1): mixed
```

## What the type checker enforces

- **Arity**: takes exactly 4 arguments.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `cairo_pattern_create_linear()`](../../../php/builtins/image/cairo_pattern_create_linear.md)
