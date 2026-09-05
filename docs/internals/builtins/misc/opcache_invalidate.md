---
title: "opcache_invalidate() — internals"
description: "Compiler internals for opcache_invalidate(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 615
---

## `opcache_invalidate()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/opcache_prelude/build.rs`:439](https://github.com/illegalstudio/elephc/blob/main/src/opcache_prelude/build.rs#L439) (`opcache_invalidate`)
- **Function symbol**: `opcache_invalidate()`


### Lowering notes

- Implemented by the compiler-injected OPcache prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function opcache_invalidate(mixed $filename, mixed $force = false): bool
```

## What the type checker enforces

- **Arity**: takes 1–2 arguments (1 optional).

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `opcache_invalidate()`](../../../php/builtins/misc/opcache_invalidate.md)
