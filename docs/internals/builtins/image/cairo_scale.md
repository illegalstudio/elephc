---
title: "cairo_scale() — internals"
description: "Compiler internals for cairo_scale(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 432
---

## `cairo_scale()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/image_prelude.rs`:13667](https://github.com/illegalstudio/elephc/blob/main/src/image_prelude.rs#L13667) (`cairo_scale`)
- **Function symbol**: `cairo_scale()`


### Lowering notes

- Implemented by the compiler-injected image prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function cairo_scale(mixed $context, float $sx, float $sy): void
```

## What the type checker enforces

- **Arity**: takes exactly 3 arguments.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `cairo_scale()`](../../../php/builtins/image/cairo_scale.md)
