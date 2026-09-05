---
title: "cairo_paint() — internals"
description: "Compiler internals for cairo_paint(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 421
---

## `cairo_paint()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/image_prelude.rs`:13599](https://github.com/illegalstudio/elephc/blob/main/src/image_prelude.rs#L13599) (`cairo_paint`)
- **Function symbol**: `cairo_paint()`


### Lowering notes

- Implemented by the compiler-injected image prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function cairo_paint(mixed $context): void
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `cairo_paint()`](../../../php/builtins/image/cairo_paint.md)
