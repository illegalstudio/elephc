---
title: "mysqli_rollback() — internals"
description: "Compiler internals for mysqli_rollback(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 152
---

## `mysqli_rollback()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/mysqli_prelude/build/procedural.rs`:312](https://github.com/illegalstudio/elephc/blob/main/src/mysqli_prelude/build/procedural.rs#L312) (`mysqli_rollback`)
- **Function symbol**: `mysqli_rollback()`


### Lowering notes

- Implemented by the compiler-injected mysqli prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function mysqli_rollback(mixed $mysql, int $flags = 0, string $name = null): bool
```

## What the type checker enforces

- **Arity**: takes 1–3 arguments (2 optional).

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `mysqli_rollback()`](../../../php/builtins/database/mysqli_rollback.md)
