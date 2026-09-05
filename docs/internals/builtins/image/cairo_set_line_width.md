---
title: "cairo_set_line_width() — internals"
description: "Compiler internals for cairo_set_line_width(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 436
---

## `cairo_set_line_width()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/image_prelude.rs`:13428](https://github.com/illegalstudio/elephc/blob/main/src/image_prelude.rs#L13428) (`cairo_set_line_width`)
- **Function symbol**: `cairo_set_line_width()`


### Lowering notes

- Implemented by the compiler-injected image prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function cairo_set_line_width(mixed $context, float $width): void
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `cairo_set_line_width()`](../../../php/builtins/image/cairo_set_line_width.md)
