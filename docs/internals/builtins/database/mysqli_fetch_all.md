---
title: "mysqli_fetch_all() — internals"
description: "Compiler internals for mysqli_fetch_all(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 114
---

## `mysqli_fetch_all()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/mysqli_prelude/build/procedural.rs`:1011](https://github.com/illegalstudio/elephc/blob/main/src/mysqli_prelude/build/procedural.rs#L1011) (`mysqli_fetch_all`)
- **Function symbol**: `mysqli_fetch_all()`


### Lowering notes

- Implemented by the compiler-injected mysqli prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function mysqli_fetch_all(mixed $result, int $mode = 2): array
```

## What the type checker enforces

- **Arity**: takes 1–2 arguments (1 optional).

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `mysqli_fetch_all()`](../../../php/builtins/database/mysqli_fetch_all.md)
