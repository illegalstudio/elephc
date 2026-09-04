---
title: "mysqli_data_seek() — internals"
description: "Compiler internals for mysqli_data_seek(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 107
---

## `mysqli_data_seek()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/mysqli_prelude/build/procedural.rs`:1089](https://github.com/illegalstudio/elephc/blob/main/src/mysqli_prelude/build/procedural.rs#L1089) (`mysqli_data_seek`)
- **Function symbol**: `mysqli_data_seek()`


### Lowering notes

- Implemented by the compiler-injected mysqli prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function mysqli_data_seek(mixed $result, int $offset): bool
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `mysqli_data_seek()`](../../../php/builtins/database/mysqli_data_seek.md)
