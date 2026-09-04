---
title: "cairo_identity_matrix() — internals"
description: "Compiler internals for cairo_identity_matrix(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 406
---

## `cairo_identity_matrix()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/image_prelude.rs`:13716](https://github.com/illegalstudio/elephc/blob/main/src/image_prelude.rs#L13716) (`cairo_identity_matrix`)
- **Function symbol**: `cairo_identity_matrix()`


### Lowering notes

- Implemented by the compiler-injected image prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function cairo_identity_matrix(mixed $context): void
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `cairo_identity_matrix()`](../../../php/builtins/image/cairo_identity_matrix.md)
