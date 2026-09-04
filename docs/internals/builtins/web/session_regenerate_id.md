---
title: "session_regenerate_id() — internals"
description: "Compiler internals for session_regenerate_id(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 863
---

## `session_regenerate_id()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/web_prelude/build.rs`:3113](https://github.com/illegalstudio/elephc/blob/main/src/web_prelude/build.rs#L3113) (`session_regenerate_id`)
- **Function symbol**: `session_regenerate_id()`


### Lowering notes

- Implemented by the compiler-injected web prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function session_regenerate_id(bool $delete_old = false): bool
```

## What the type checker enforces

- **Arity**: takes 0–1 arguments (1 optional).

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `session_regenerate_id()`](../../../php/builtins/web/session_regenerate_id.md)
