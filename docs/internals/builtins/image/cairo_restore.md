---
title: "cairo_restore() — internals"
description: "Compiler internals for cairo_restore(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 429
---

## `cairo_restore()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/image_prelude.rs`:13376](https://github.com/illegalstudio/elephc/blob/main/src/image_prelude.rs#L13376) (`cairo_restore`)
- **Function symbol**: `cairo_restore()`


### Lowering notes

- Implemented by the compiler-injected image prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function cairo_restore(mixed $context): void
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `cairo_restore()`](../../../php/builtins/image/cairo_restore.md)
