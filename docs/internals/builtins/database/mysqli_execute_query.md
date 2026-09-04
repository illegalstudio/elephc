---
title: "mysqli_execute_query() — internals"
description: "Compiler internals for mysqli_execute_query(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 113
---

## `mysqli_execute_query()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/mysqli_prelude/build/procedural.rs`:1206](https://github.com/illegalstudio/elephc/blob/main/src/mysqli_prelude/build/procedural.rs#L1206) (`mysqli_execute_query`)
- **Function symbol**: `mysqli_execute_query()`


### Lowering notes

- Implemented by the compiler-injected mysqli prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function mysqli_execute_query(mixed $mysql, string $query, mixed $params = null): mixed
```

## What the type checker enforces

- **Arity**: takes 2–3 arguments (1 optional).

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `mysqli_execute_query()`](../../../php/builtins/database/mysqli_execute_query.md)
