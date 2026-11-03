---
title: "mysqli_stmt_attr_set() - internals"
description: "Compiler internals for mysqli_stmt_attr_set(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 178
---

## `mysqli_stmt_attr_set()` - internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/mysqli_prelude/build/procedural.rs`:1842](https://github.com/illegalstudio/elephc/blob/main/src/mysqli_prelude/build/procedural.rs#L1842) (`mysqli_stmt_attr_set`)
- **Function symbol**: `mysqli_stmt_attr_set()`


### Lowering notes

- Implemented by the compiler-injected mysqli prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function mysqli_stmt_attr_set(mixed $statement, int $attribute, int $value): bool
```

## What the type checker enforces

- **Arity**: takes exactly 3 arguments.

## Eval interpreter (magician)

_Not callable from eval'd code - the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `mysqli_stmt_attr_set()`](../../../php/builtins/database/mysqli_stmt_attr_set.md)
