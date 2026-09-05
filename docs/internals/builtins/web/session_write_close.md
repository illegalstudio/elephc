---
title: "session_write_close() — internals"
description: "Compiler internals for session_write_close(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 872
---

## `session_write_close()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/web_prelude/build.rs`:2826](https://github.com/illegalstudio/elephc/blob/main/src/web_prelude/build.rs#L2826) (`session_write_close`)
- **Function symbol**: `session_write_close()`


### Lowering notes

- Implemented by the compiler-injected web prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function session_write_close(): bool
```

## What the type checker enforces

- **Arity**: takes no arguments.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `session_write_close()`](../../../php/builtins/web/session_write_close.md)
