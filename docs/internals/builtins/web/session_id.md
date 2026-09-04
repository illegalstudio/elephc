---
title: "session_id() — internals"
description: "Compiler internals for session_id(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 860
---

## `session_id()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/web_prelude/build.rs`:2950](https://github.com/illegalstudio/elephc/blob/main/src/web_prelude/build.rs#L2950) (`session_id`)
- **Function symbol**: `session_id()`


### Lowering notes

- Implemented by the compiler-injected web prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function session_id(string $id = null): mixed
```

## What the type checker enforces

- **Arity**: takes 0–1 arguments (1 optional).

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `session_id()`](../../../php/builtins/web/session_id.md)
