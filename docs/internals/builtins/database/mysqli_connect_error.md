---
title: "mysqli_connect_error() — internals"
description: "Compiler internals for mysqli_connect_error(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 106
---

## `mysqli_connect_error()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/mysqli_prelude/build/procedural.rs`:597](https://github.com/illegalstudio/elephc/blob/main/src/mysqli_prelude/build/procedural.rs#L597) (`mysqli_connect_error`)
- **Function symbol**: `mysqli_connect_error()`


### Lowering notes

- Implemented by the compiler-injected mysqli prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function mysqli_connect_error(): string
```

## What the type checker enforces

- **Arity**: takes no arguments.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `mysqli_connect_error()`](../../../php/builtins/database/mysqli_connect_error.md)
