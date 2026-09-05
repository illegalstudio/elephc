---
title: "php_sapi_name() — internals"
description: "Compiler internals for php_sapi_name(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 620
---

## `php_sapi_name()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/version_prelude.rs`:50](https://github.com/illegalstudio/elephc/blob/main/src/version_prelude.rs#L50) (`php_sapi_name`)
- **Function symbol**: `php_sapi_name()`


### Lowering notes

- Implemented by the compiler-injected version prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function php_sapi_name(): string
```

## What the type checker enforces

- **Arity**: takes no arguments.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `php_sapi_name()`](../../../php/builtins/misc/php_sapi_name.md)
