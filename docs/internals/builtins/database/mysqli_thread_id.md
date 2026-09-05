---
title: "mysqli_thread_id() — internals"
description: "Compiler internals for mysqli_thread_id(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 178
---

## `mysqli_thread_id()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/mysqli_prelude/build/procedural.rs`:568](https://github.com/illegalstudio/elephc/blob/main/src/mysqli_prelude/build/procedural.rs#L568) (`mysqli_thread_id`)
- **Function symbol**: `mysqli_thread_id()`


### Lowering notes

- Implemented by the compiler-injected mysqli prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function mysqli_thread_id(mixed $mysql): int
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `mysqli_thread_id()`](../../../php/builtins/database/mysqli_thread_id.md)
