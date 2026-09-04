---
title: "mysqli_init() — internals"
description: "Compiler internals for mysqli_init(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 136
---

## `mysqli_init()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/mysqli_prelude/build/procedural.rs`:97](https://github.com/illegalstudio/elephc/blob/main/src/mysqli_prelude/build/procedural.rs#L97) (`mysqli_init`)
- **Function symbol**: `mysqli_init()`


### Lowering notes

- Implemented by the compiler-injected mysqli prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function mysqli_init(): mixed
```

## What the type checker enforces

- **Arity**: takes no arguments.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `mysqli_init()`](../../../php/builtins/database/mysqli_init.md)
