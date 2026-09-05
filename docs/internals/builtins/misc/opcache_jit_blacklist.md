---
title: "opcache_jit_blacklist() — internals"
description: "Compiler internals for opcache_jit_blacklist(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 618
---

## `opcache_jit_blacklist()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/opcache_prelude/build.rs`:540](https://github.com/illegalstudio/elephc/blob/main/src/opcache_prelude/build.rs#L540) (`opcache_jit_blacklist`)
- **Function symbol**: `opcache_jit_blacklist()`


### Lowering notes

- Implemented by the compiler-injected OPcache prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function opcache_jit_blacklist(mixed $closure): void
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `opcache_jit_blacklist()`](../../../php/builtins/misc/opcache_jit_blacklist.md)
