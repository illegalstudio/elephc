---
title: "opcache_compile_file() — internals"
description: "Compiler internals for opcache_compile_file(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 612
---

## `opcache_compile_file()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/opcache_prelude/build.rs`:503](https://github.com/illegalstudio/elephc/blob/main/src/opcache_prelude/build.rs#L503) (`opcache_compile_file`)
- **Function symbol**: `opcache_compile_file()`


### Lowering notes

- Implemented by the compiler-injected OPcache prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function opcache_compile_file(mixed $filename): bool
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `opcache_compile_file()`](../../../php/builtins/misc/opcache_compile_file.md)
