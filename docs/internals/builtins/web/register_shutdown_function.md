---
title: "register_shutdown_function() - internals"
description: "Compiler internals for register_shutdown_function(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 1117
---

## `register_shutdown_function()` - internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/error_handling_prelude.rs`:353](https://github.com/illegalstudio/elephc/blob/main/src/error_handling_prelude.rs#L353) (`register_shutdown_function`)
- **Function symbol**: `register_shutdown_function()`


### Lowering notes

- Implemented by the compiler-injected error-handling prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function register_shutdown_function(callable $callback, ...$args): void
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.
- **Variadic**: collects excess arguments into `$args`.

## Eval interpreter (magician)

_Not callable from eval'd code - the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `register_shutdown_function()`](../../../php/builtins/web/register_shutdown_function.md)
