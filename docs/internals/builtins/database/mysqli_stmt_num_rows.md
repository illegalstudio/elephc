---
title: "mysqli_stmt_num_rows() — internals"
description: "Compiler internals for mysqli_stmt_num_rows(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 171
---

## `mysqli_stmt_num_rows()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/mysqli_prelude/build/procedural.rs`:1410](https://github.com/illegalstudio/elephc/blob/main/src/mysqli_prelude/build/procedural.rs#L1410) (`mysqli_stmt_num_rows`)
- **Function symbol**: `mysqli_stmt_num_rows()`


### Lowering notes

- Implemented by the compiler-injected mysqli prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function mysqli_stmt_num_rows(mixed $statement): int
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `mysqli_stmt_num_rows()`](../../../php/builtins/database/mysqli_stmt_num_rows.md)
