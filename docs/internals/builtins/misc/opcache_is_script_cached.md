---
title: "opcache_is_script_cached() — internals"
description: "Compiler internals for opcache_is_script_cached(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 616
---

## `opcache_is_script_cached()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/opcache_prelude/build.rs`:374](https://github.com/illegalstudio/elephc/blob/main/src/opcache_prelude/build.rs#L374) (`opcache_is_script_cached`)
- **Function symbol**: `opcache_is_script_cached()`


### Lowering notes

- Implemented by the compiler-injected OPcache prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function opcache_is_script_cached(mixed $filename): bool
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `opcache_is_script_cached()`](../../../php/builtins/misc/opcache_is_script_cached.md)
