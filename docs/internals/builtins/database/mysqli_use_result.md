---
title: "mysqli_use_result() — internals"
description: "Compiler internals for mysqli_use_result(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 180
---

## `mysqli_use_result()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/mysqli_prelude/build/procedural.rs`:913](https://github.com/illegalstudio/elephc/blob/main/src/mysqli_prelude/build/procedural.rs#L913) (`mysqli_use_result`)
- **Function symbol**: `mysqli_use_result()`


### Lowering notes

- Implemented by the compiler-injected mysqli prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function mysqli_use_result(mixed $mysql): mixed
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `mysqli_use_result()`](../../../php/builtins/database/mysqli_use_result.md)
