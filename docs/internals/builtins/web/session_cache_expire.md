---
title: "session_cache_expire() — internals"
description: "Compiler internals for session_cache_expire(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 851
---

## `session_cache_expire()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/web_prelude/build.rs`:3480](https://github.com/illegalstudio/elephc/blob/main/src/web_prelude/build.rs#L3480) (`session_cache_expire`)
- **Function symbol**: `session_cache_expire()`


### Lowering notes

- Implemented by the compiler-injected web prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function session_cache_expire(?int $value = null): mixed
```

## What the type checker enforces

- **Arity**: takes 0–1 arguments (1 optional).

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `session_cache_expire()`](../../../php/builtins/web/session_cache_expire.md)
