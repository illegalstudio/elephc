---
title: "mysqli_select_db() — internals"
description: "Compiler internals for mysqli_select_db(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 154
---

## `mysqli_select_db()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/mysqli_prelude/build/procedural.rs`:171](https://github.com/illegalstudio/elephc/blob/main/src/mysqli_prelude/build/procedural.rs#L171) (`mysqli_select_db`)
- **Function symbol**: `mysqli_select_db()`


### Lowering notes

- Implemented by the compiler-injected mysqli prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function mysqli_select_db(mixed $mysql, string $database): bool
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `mysqli_select_db()`](../../../php/builtins/database/mysqli_select_db.md)
