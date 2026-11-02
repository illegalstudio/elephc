---
title: "mysqli_get_connection_stats() - internals"
description: "Compiler internals for mysqli_get_connection_stats(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 139
---

## `mysqli_get_connection_stats()` - internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/mysqli_prelude/build/procedural.rs`:678](https://github.com/illegalstudio/elephc/blob/main/src/mysqli_prelude/build/procedural.rs#L678) (`mysqli_get_connection_stats`)
- **Function symbol**: `mysqli_get_connection_stats()`


### Lowering notes

- Implemented by the compiler-injected mysqli prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function mysqli_get_connection_stats(mixed $mysql): array
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

_Not callable from eval'd code - the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `mysqli_get_connection_stats()`](../../../php/builtins/database/mysqli_get_connection_stats.md)
