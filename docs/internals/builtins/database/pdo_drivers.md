---
title: "pdo_drivers() — internals"
description: "Compiler internals for pdo_drivers(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 182
---

## `pdo_drivers()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/pdo_prelude/build.rs`:7675](https://github.com/illegalstudio/elephc/blob/main/src/pdo_prelude/build.rs#L7675) (`pdo_drivers`)
- **Function symbol**: `pdo_drivers()`


### Lowering notes

- Implemented by the compiler-injected PDO prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function pdo_drivers(): array
```

## What the type checker enforces

- **Arity**: takes no arguments.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `pdo_drivers()`](../../../php/builtins/database/pdo_drivers.md)
