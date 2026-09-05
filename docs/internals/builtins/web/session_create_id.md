---
title: "session_create_id() — internals"
description: "Compiler internals for session_create_id(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 854
---

## `session_create_id()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/web_prelude/build.rs`:3298](https://github.com/illegalstudio/elephc/blob/main/src/web_prelude/build.rs#L3298) (`session_create_id`)
- **Function symbol**: `session_create_id()`


### Lowering notes

- Implemented by the compiler-injected web prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function session_create_id(string $prefix = ''): mixed
```

## What the type checker enforces

- **Arity**: takes 0–1 arguments (1 optional).

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `session_create_id()`](../../../php/builtins/web/session_create_id.md)
