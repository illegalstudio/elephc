---
title: "mysqli_fetch_assoc() — internals"
description: "Compiler internals for mysqli_fetch_assoc(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 116
---

## `mysqli_fetch_assoc()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/mysqli_prelude/build/procedural.rs`:932](https://github.com/illegalstudio/elephc/blob/main/src/mysqli_prelude/build/procedural.rs#L932) (`mysqli_fetch_assoc`)
- **Function symbol**: `mysqli_fetch_assoc()`


### Lowering notes

- Implemented by the compiler-injected mysqli prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function mysqli_fetch_assoc(mixed $result): mixed
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `mysqli_fetch_assoc()`](../../../php/builtins/database/mysqli_fetch_assoc.md)
