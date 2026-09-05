---
title: "opcache_get_configuration() — internals"
description: "Compiler internals for opcache_get_configuration(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 613
---

## `opcache_get_configuration()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/opcache_prelude/build.rs`:128](https://github.com/illegalstudio/elephc/blob/main/src/opcache_prelude/build.rs#L128) (`opcache_get_configuration`)
- **Function symbol**: `opcache_get_configuration()`


### Lowering notes

- Implemented by the compiler-injected OPcache prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function opcache_get_configuration(): array
```

## What the type checker enforces

- **Arity**: takes no arguments.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `opcache_get_configuration()`](../../../php/builtins/misc/opcache_get_configuration.md)
