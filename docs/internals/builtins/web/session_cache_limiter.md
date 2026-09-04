---
title: "session_cache_limiter() — internals"
description: "Compiler internals for session_cache_limiter(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 852
---

## `session_cache_limiter()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/web_prelude/build.rs`:3452](https://github.com/illegalstudio/elephc/blob/main/src/web_prelude/build.rs#L3452) (`session_cache_limiter`)
- **Function symbol**: `session_cache_limiter()`


### Lowering notes

- Implemented by the compiler-injected web prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function session_cache_limiter(string $value = null): mixed
```

## What the type checker enforces

- **Arity**: takes 0–1 arguments (1 optional).

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `session_cache_limiter()`](../../../php/builtins/web/session_cache_limiter.md)
