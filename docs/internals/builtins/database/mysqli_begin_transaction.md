---
title: "mysqli_begin_transaction() — internals"
description: "Compiler internals for mysqli_begin_transaction(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 100
---

## `mysqli_begin_transaction()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/mysqli_prelude/build/procedural.rs`:270](https://github.com/illegalstudio/elephc/blob/main/src/mysqli_prelude/build/procedural.rs#L270) (`mysqli_begin_transaction`)
- **Function symbol**: `mysqli_begin_transaction()`


### Lowering notes

- Implemented by the compiler-injected mysqli prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function mysqli_begin_transaction(mixed $mysql, int $flags = 0, string $name = null): bool
```

## What the type checker enforces

- **Arity**: takes 1–3 arguments (2 optional).

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `mysqli_begin_transaction()`](../../../php/builtins/database/mysqli_begin_transaction.md)
