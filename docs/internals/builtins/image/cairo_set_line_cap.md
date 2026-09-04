---
title: "cairo_set_line_cap() — internals"
description: "Compiler internals for cairo_set_line_cap(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 434
---

## `cairo_set_line_cap()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/image_prelude.rs`:13440](https://github.com/illegalstudio/elephc/blob/main/src/image_prelude.rs#L13440) (`cairo_set_line_cap`)
- **Function symbol**: `cairo_set_line_cap()`


### Lowering notes

- Implemented by the compiler-injected image prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function cairo_set_line_cap(mixed $context, int $lineCap): void
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `cairo_set_line_cap()`](../../../php/builtins/image/cairo_set_line_cap.md)
