---
title: "mysqli_fetch_fields() — internals"
description: "Compiler internals for mysqli_fetch_fields(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 120
---

## `mysqli_fetch_fields()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/mysqli_prelude/build/procedural.rs`:1128](https://github.com/illegalstudio/elephc/blob/main/src/mysqli_prelude/build/procedural.rs#L1128) (`mysqli_fetch_fields`)
- **Function symbol**: `mysqli_fetch_fields()`


### Lowering notes

- Implemented by the compiler-injected mysqli prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function mysqli_fetch_fields(mixed $result): mixed
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `mysqli_fetch_fields()`](../../../php/builtins/database/mysqli_fetch_fields.md)
