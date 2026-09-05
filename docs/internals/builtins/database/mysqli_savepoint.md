---
title: "mysqli_savepoint() — internals"
description: "Compiler internals for mysqli_savepoint(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 153
---

## `mysqli_savepoint()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/mysqli_prelude/build/procedural.rs`:333](https://github.com/illegalstudio/elephc/blob/main/src/mysqli_prelude/build/procedural.rs#L333) (`mysqli_savepoint`)
- **Function symbol**: `mysqli_savepoint()`


### Lowering notes

- Implemented by the compiler-injected mysqli prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function mysqli_savepoint(mixed $mysql, string $name): bool
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `mysqli_savepoint()`](../../../php/builtins/database/mysqli_savepoint.md)
