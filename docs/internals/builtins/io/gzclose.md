---
title: "gzclose() — internals"
description: "Compiler internals for gzclose(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 200
---

## `gzclose()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_surfaces.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_surfaces.rs)
- **Lowering**: [`src/gz_prelude.rs`:97](https://github.com/illegalstudio/elephc/blob/main/src/gz_prelude.rs#L97) (`gzclose`)
- **Function symbol**: `gzclose()`


### Lowering notes

- Implemented by the compiler-injected gz prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function gzclose(mixed $stream): bool
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `gzclose()`](../../../php/builtins/io/gzclose.md)
