---
title: "error_log() — internals"
description: "Compiler internals for error_log(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 846
---

## `error_log()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/web_prelude/build.rs`:1761](https://github.com/illegalstudio/elephc/blob/main/src/web_prelude/build.rs#L1761) (`error_log`)
- **Function symbol**: `error_log()`


### Lowering notes

- Implemented by the compiler-injected web prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function error_log(string $message, int $message_type = 0, string $destination = null, string $additional_headers = null): bool
```

## What the type checker enforces

- **Arity**: takes 1–4 arguments (3 optional).

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `error_log()`](../../../php/builtins/web/error_log.md)
