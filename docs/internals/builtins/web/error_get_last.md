---
title: "error_get_last() - internals"
description: "Compiler internals for error_get_last(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 1113
---

## `error_get_last()` - internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/error_handling_prelude.rs`:121](https://github.com/illegalstudio/elephc/blob/main/src/error_handling_prelude.rs#L121) (`error_get_last`)
- **Function symbol**: `error_get_last()`


### Lowering notes

- Implemented by the compiler-injected error-handling prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function error_get_last(): ?array
```

## What the type checker enforces

- **Arity**: takes no arguments.

## Eval interpreter (magician)

_Not callable from eval'd code - the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `error_get_last()`](../../../php/builtins/web/error_get_last.md)
