---
title: "mysqli_error() — internals"
description: "Compiler internals for mysqli_error(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 109
---

## `mysqli_error()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/mysqli_prelude/build/procedural.rs`:634](https://github.com/illegalstudio/elephc/blob/main/src/mysqli_prelude/build/procedural.rs#L634) (`mysqli_error`)
- **Function symbol**: `mysqli_error()`


### Lowering notes

- Implemented by the compiler-injected mysqli prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function mysqli_error(mixed $mysql): string
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `mysqli_error()`](../../../php/builtins/database/mysqli_error.md)
