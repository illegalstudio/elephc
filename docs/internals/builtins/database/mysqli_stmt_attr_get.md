---
title: "mysqli_stmt_attr_get() - internals"
description: "Compiler internals for mysqli_stmt_attr_get(): lowering path, type checks, and runtime helpers."
---

## `mysqli_stmt_attr_get()` - internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/mysqli_prelude/build/procedural.rs`](https://github.com/illegalstudio/elephc/blob/main/src/mysqli_prelude/build/procedural.rs) (`mysqli_stmt_attr_get`)
- **Function symbol**: `mysqli_stmt_attr_get()`


### Lowering notes

- Implemented by the compiler-injected mysqli prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function mysqli_stmt_attr_get(mixed $statement, int $attribute): int
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Eval interpreter (magician)

_Not callable from eval'd code - the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `mysqli_stmt_attr_get()`](../../../php/builtins/database/mysqli_stmt_attr_get.md)
