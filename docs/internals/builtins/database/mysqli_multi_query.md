---
title: "mysqli_multi_query() — internals"
description: "Compiler internals for mysqli_multi_query(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 139
---

## `mysqli_multi_query()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/mysqli_prelude/build/procedural.rs`:835](https://github.com/illegalstudio/elephc/blob/main/src/mysqli_prelude/build/procedural.rs#L835) (`mysqli_multi_query`)
- **Function symbol**: `mysqli_multi_query()`


### Lowering notes

- Implemented by the compiler-injected mysqli prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function mysqli_multi_query(mixed $mysql, string $query): bool
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `mysqli_multi_query()`](../../../php/builtins/database/mysqli_multi_query.md)
