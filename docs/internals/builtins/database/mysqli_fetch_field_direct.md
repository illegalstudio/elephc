---
title: "mysqli_fetch_field_direct() — internals"
description: "Compiler internals for mysqli_fetch_field_direct(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 119
---

## `mysqli_fetch_field_direct()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/mysqli_prelude/build/procedural.rs`:1147](https://github.com/illegalstudio/elephc/blob/main/src/mysqli_prelude/build/procedural.rs#L1147) (`mysqli_fetch_field_direct`)
- **Function symbol**: `mysqli_fetch_field_direct()`


### Lowering notes

- Implemented by the compiler-injected mysqli prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function mysqli_fetch_field_direct(mixed $result, int $index): mixed
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `mysqli_fetch_field_direct()`](../../../php/builtins/database/mysqli_fetch_field_direct.md)
