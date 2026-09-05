---
title: "session_start() — internals"
description: "Compiler internals for session_start(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 869
---

## `session_start()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/web_prelude/build.rs`:1886](https://github.com/illegalstudio/elephc/blob/main/src/web_prelude/build.rs#L1886) (`session_start`)
- **Function symbol**: `session_start()`


### Lowering notes

- Implemented by the compiler-injected web prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function session_start(mixed $options = []): bool
```

## What the type checker enforces

- **Arity**: takes 0–1 arguments (1 optional).

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `session_start()`](../../../php/builtins/web/session_start.md)
