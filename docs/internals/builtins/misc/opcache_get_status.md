---
title: "opcache_get_status() — internals"
description: "Compiler internals for opcache_get_status(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 614
---

## `opcache_get_status()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/opcache_prelude/build.rs`:348](https://github.com/illegalstudio/elephc/blob/main/src/opcache_prelude/build.rs#L348) (`opcache_get_status`)
- **Function symbol**: `opcache_get_status()`


### Lowering notes

- Implemented by the compiler-injected OPcache prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function opcache_get_status(mixed $include_scripts = true): mixed
```

## What the type checker enforces

- **Arity**: takes 0–1 arguments (1 optional).

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `opcache_get_status()`](../../../php/builtins/misc/opcache_get_status.md)
