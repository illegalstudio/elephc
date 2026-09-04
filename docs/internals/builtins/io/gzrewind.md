---
title: "gzrewind() — internals"
description: "Compiler internals for gzrewind(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 209
---

## `gzrewind()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_surfaces.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_surfaces.rs)
- **Lowering**: [`src/gz_prelude.rs`:135](https://github.com/illegalstudio/elephc/blob/main/src/gz_prelude.rs#L135) (`gzrewind`)
- **Function symbol**: `gzrewind()`


### Lowering notes

- Implemented by the compiler-injected gz prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function gzrewind(mixed $stream): bool
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `gzrewind()`](../../../php/builtins/io/gzrewind.md)
