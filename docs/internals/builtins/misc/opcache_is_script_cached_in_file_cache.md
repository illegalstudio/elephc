---
title: "opcache_is_script_cached_in_file_cache() — internals"
description: "Compiler internals for opcache_is_script_cached_in_file_cache(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 617
---

## `opcache_is_script_cached_in_file_cache()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/opcache_prelude/build.rs`:513](https://github.com/illegalstudio/elephc/blob/main/src/opcache_prelude/build.rs#L513) (`opcache_is_script_cached_in_file_cache`)
- **Function symbol**: `opcache_is_script_cached_in_file_cache()`


### Lowering notes

- Implemented by the compiler-injected OPcache prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function opcache_is_script_cached_in_file_cache(mixed $filename): bool
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `opcache_is_script_cached_in_file_cache()`](../../../php/builtins/misc/opcache_is_script_cached_in_file_cache.md)
