---
title: "mysqli_connect_errno() — internals"
description: "Compiler internals for mysqli_connect_errno(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 105
---

## `mysqli_connect_errno()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/mysqli_prelude/build/procedural.rs`:587](https://github.com/illegalstudio/elephc/blob/main/src/mysqli_prelude/build/procedural.rs#L587) (`mysqli_connect_errno`)
- **Function symbol**: `mysqli_connect_errno()`


### Lowering notes

- Implemented by the compiler-injected mysqli prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function mysqli_connect_errno(): int
```

## What the type checker enforces

- **Arity**: takes no arguments.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `mysqli_connect_errno()`](../../../php/builtins/database/mysqli_connect_errno.md)
