---
title: "session_set_save_handler() — internals"
description: "Compiler internals for session_set_save_handler(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 868
---

## `session_set_save_handler()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/web_prelude/build.rs`:3744](https://github.com/illegalstudio/elephc/blob/main/src/web_prelude/build.rs#L3744) (`session_set_save_handler`)
- **Function symbol**: `session_set_save_handler()`


### Lowering notes

- Implemented by the compiler-injected web prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function session_set_save_handler(mixed $handler_or_open = null, mixed $register_or_close = true, mixed $read = null, mixed $write = null, mixed $destroy = null, mixed $gc = null, mixed $create_sid = null, mixed $validate_id = null, mixed $update_timestamp = null): bool
```

## What the type checker enforces

- **Arity**: takes 0–9 arguments (9 optional).

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `session_set_save_handler()`](../../../php/builtins/web/session_set_save_handler.md)
