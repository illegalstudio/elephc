---
title: "cairo_new_path() — internals"
description: "Compiler internals for cairo_new_path(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 419
---

## `cairo_new_path()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/image_prelude.rs`:13577](https://github.com/illegalstudio/elephc/blob/main/src/image_prelude.rs#L13577) (`cairo_new_path`)
- **Function symbol**: `cairo_new_path()`


### Lowering notes

- Implemented by the compiler-injected image prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function cairo_new_path(mixed $context): void
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `cairo_new_path()`](../../../php/builtins/image/cairo_new_path.md)
