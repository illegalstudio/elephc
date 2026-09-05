---
title: "session_register_shutdown() — internals"
description: "Compiler internals for session_register_shutdown(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 864
---

## `session_register_shutdown()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/web_prelude/build.rs`:3697](https://github.com/illegalstudio/elephc/blob/main/src/web_prelude/build.rs#L3697) (`session_register_shutdown`)
- **Function symbol**: `session_register_shutdown()`


### Lowering notes

- Implemented by the compiler-injected web prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function session_register_shutdown(): void
```

## What the type checker enforces

- **Arity**: takes no arguments.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `session_register_shutdown()`](../../../php/builtins/web/session_register_shutdown.md)
