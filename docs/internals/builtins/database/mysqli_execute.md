---
title: "mysqli_execute() — internals"
description: "Compiler internals for mysqli_execute(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 112
---

## `mysqli_execute()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/mysqli_prelude/build/procedural.rs`:1564](https://github.com/illegalstudio/elephc/blob/main/src/mysqli_prelude/build/procedural.rs#L1564) (`mysqli_execute`)
- **Function symbol**: `mysqli_execute()`


### Lowering notes

- Implemented by the compiler-injected mysqli prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function mysqli_execute(mixed $statement): bool
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `mysqli_execute()`](../../../php/builtins/database/mysqli_execute.md)
