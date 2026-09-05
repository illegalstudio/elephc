---
title: "mysqli_stmt_store_result() — internals"
description: "Compiler internals for mysqli_stmt_store_result(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 176
---

## `mysqli_stmt_store_result()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/mysqli_prelude/build/procedural.rs`:1296](https://github.com/illegalstudio/elephc/blob/main/src/mysqli_prelude/build/procedural.rs#L1296) (`mysqli_stmt_store_result`)
- **Function symbol**: `mysqli_stmt_store_result()`


### Lowering notes

- Implemented by the compiler-injected mysqli prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function mysqli_stmt_store_result(mixed $statement): bool
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `mysqli_stmt_store_result()`](../../../php/builtins/database/mysqli_stmt_store_result.md)
