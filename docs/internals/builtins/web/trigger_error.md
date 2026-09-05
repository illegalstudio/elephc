---
title: "trigger_error() — internals"
description: "Compiler internals for trigger_error(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 875
---

## `trigger_error()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/web_prelude/build.rs`:1828](https://github.com/illegalstudio/elephc/blob/main/src/web_prelude/build.rs#L1828) (`trigger_error`)
- **Function symbol**: `trigger_error()`


### Lowering notes

- Implemented by the compiler-injected web prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function trigger_error(string $message, int $error_level = E_USER_NOTICE): bool
```

## What the type checker enforces

- **Arity**: takes 1–2 arguments (1 optional).

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `trigger_error()`](../../../php/builtins/web/trigger_error.md)
