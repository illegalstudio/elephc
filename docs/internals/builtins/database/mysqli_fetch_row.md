---
title: "mysqli_fetch_row() — internals"
description: "Compiler internals for mysqli_fetch_row(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 123
---

## `mysqli_fetch_row()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/mysqli_prelude/build/procedural.rs`:951](https://github.com/illegalstudio/elephc/blob/main/src/mysqli_prelude/build/procedural.rs#L951) (`mysqli_fetch_row`)
- **Function symbol**: `mysqli_fetch_row()`


### Lowering notes

- Implemented by the compiler-injected mysqli prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function mysqli_fetch_row(mixed $result): mixed
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `mysqli_fetch_row()`](../../../php/builtins/database/mysqli_fetch_row.md)
