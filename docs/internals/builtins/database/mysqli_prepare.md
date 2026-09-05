---
title: "mysqli_prepare() — internals"
description: "Compiler internals for mysqli_prepare(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 145
---

## `mysqli_prepare()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/mysqli_prelude/build/procedural.rs`:1186](https://github.com/illegalstudio/elephc/blob/main/src/mysqli_prelude/build/procedural.rs#L1186) (`mysqli_prepare`)
- **Function symbol**: `mysqli_prepare()`


### Lowering notes

- Implemented by the compiler-injected mysqli prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function mysqli_prepare(mixed $mysql, string $query): mixed
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `mysqli_prepare()`](../../../php/builtins/database/mysqli_prepare.md)
