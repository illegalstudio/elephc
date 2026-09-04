---
title: "mysqli_next_result() — internals"
description: "Compiler internals for mysqli_next_result(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 140
---

## `mysqli_next_result()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/mysqli_prelude/build/procedural.rs`:874](https://github.com/illegalstudio/elephc/blob/main/src/mysqli_prelude/build/procedural.rs#L874) (`mysqli_next_result`)
- **Function symbol**: `mysqli_next_result()`


### Lowering notes

- Implemented by the compiler-injected mysqli prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function mysqli_next_result(mixed $mysql): bool
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `mysqli_next_result()`](../../../php/builtins/database/mysqli_next_result.md)
