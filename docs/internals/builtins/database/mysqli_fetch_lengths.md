---
title: "mysqli_fetch_lengths() — internals"
description: "Compiler internals for mysqli_fetch_lengths(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 121
---

## `mysqli_fetch_lengths()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/mysqli_prelude/build/procedural.rs`:1602](https://github.com/illegalstudio/elephc/blob/main/src/mysqli_prelude/build/procedural.rs#L1602) (`mysqli_fetch_lengths`)
- **Function symbol**: `mysqli_fetch_lengths()`


### Lowering notes

- Implemented by the compiler-injected mysqli prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function mysqli_fetch_lengths(mixed $result): mixed
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `mysqli_fetch_lengths()`](../../../php/builtins/database/mysqli_fetch_lengths.md)
