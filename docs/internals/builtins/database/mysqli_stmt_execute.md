---
title: "mysqli_stmt_execute() — internals"
description: "Compiler internals for mysqli_stmt_execute(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 165
---

## `mysqli_stmt_execute()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/mysqli_prelude/build/procedural.rs`:1257](https://github.com/illegalstudio/elephc/blob/main/src/mysqli_prelude/build/procedural.rs#L1257) (`mysqli_stmt_execute`)
- **Function symbol**: `mysqli_stmt_execute()`


### Lowering notes

- Implemented by the compiler-injected mysqli prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function mysqli_stmt_execute(mixed $statement, ?array $params = null): bool
```

## What the type checker enforces

- **Arity**: takes 1–2 arguments (1 optional).

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `mysqli_stmt_execute()`](../../../php/builtins/database/mysqli_stmt_execute.md)
