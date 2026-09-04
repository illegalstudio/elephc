---
title: "mysqli_stmt_prepare() — internals"
description: "Compiler internals for mysqli_stmt_prepare(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 173
---

## `mysqli_stmt_prepare()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/mysqli_prelude/build/procedural.rs`:1543](https://github.com/illegalstudio/elephc/blob/main/src/mysqli_prelude/build/procedural.rs#L1543) (`mysqli_stmt_prepare`)
- **Function symbol**: `mysqli_stmt_prepare()`


### Lowering notes

- Implemented by the compiler-injected mysqli prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function mysqli_stmt_prepare(mixed $statement, string $query): bool
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `mysqli_stmt_prepare()`](../../../php/builtins/database/mysqli_stmt_prepare.md)
