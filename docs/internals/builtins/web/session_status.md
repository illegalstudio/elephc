---
title: "session_status() — internals"
description: "Compiler internals for session_status(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 870
---

## `session_status()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/web_prelude/build.rs`:3017](https://github.com/illegalstudio/elephc/blob/main/src/web_prelude/build.rs#L3017) (`session_status`)
- **Function symbol**: `session_status()`


### Lowering notes

- Implemented by the compiler-injected web prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function session_status(): int
```

## What the type checker enforces

- **Arity**: takes no arguments.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `session_status()`](../../../php/builtins/web/session_status.md)
