---
title: "mysqli_error_list() — internals"
description: "Compiler internals for mysqli_error_list(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 110
---

## `mysqli_error_list()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/mysqli_prelude/build/procedural.rs`:653](https://github.com/illegalstudio/elephc/blob/main/src/mysqli_prelude/build/procedural.rs#L653) (`mysqli_error_list`)
- **Function symbol**: `mysqli_error_list()`


### Lowering notes

- Implemented by the compiler-injected mysqli prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function mysqli_error_list(mixed $mysql): array
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `mysqli_error_list()`](../../../php/builtins/database/mysqli_error_list.md)
