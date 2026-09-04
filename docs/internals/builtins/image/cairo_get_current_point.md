---
title: "cairo_get_current_point() — internals"
description: "Compiler internals for cairo_get_current_point(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 405
---

## `cairo_get_current_point()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/image_prelude.rs`:13727](https://github.com/illegalstudio/elephc/blob/main/src/image_prelude.rs#L13727) (`cairo_get_current_point`)
- **Function symbol**: `cairo_get_current_point()`


### Lowering notes

- Implemented by the compiler-injected image prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function cairo_get_current_point(mixed $context): mixed
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `cairo_get_current_point()`](../../../php/builtins/image/cairo_get_current_point.md)
