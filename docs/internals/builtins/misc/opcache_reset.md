---
title: "opcache_reset() — internals"
description: "Compiler internals for opcache_reset(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 619
---

## `opcache_reset()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/opcache_prelude/build.rs`:153](https://github.com/illegalstudio/elephc/blob/main/src/opcache_prelude/build.rs#L153) (`opcache_reset`)
- **Function symbol**: `opcache_reset()`


### Lowering notes

- Implemented by the compiler-injected OPcache prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function opcache_reset(): bool
```

## What the type checker enforces

- **Arity**: takes no arguments.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `opcache_reset()`](../../../php/builtins/misc/opcache_reset.md)
