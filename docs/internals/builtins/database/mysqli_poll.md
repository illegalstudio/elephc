---
title: "mysqli_poll() - internals"
description: "Compiler internals for mysqli_poll(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 157
---

## `mysqli_poll()` - internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/mysqli_prelude/build/procedural.rs`:1141](https://github.com/illegalstudio/elephc/blob/main/src/mysqli_prelude/build/procedural.rs#L1141) (`mysqli_poll`)
- **Function symbol**: `mysqli_poll()`


### Lowering notes

- Implemented by the compiler-injected mysqli prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function mysqli_poll(?array $read, ?array $error, array $reject, int $seconds, int $microseconds = 0): mixed
```

## What the type checker enforces

- **Arity**: takes 4–5 arguments (1 optional).
- **By-reference parameters**: `$read`, `$error`, `$reject`.

## Eval interpreter (magician)

_Not callable from eval'd code - the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `mysqli_poll()`](../../../php/builtins/database/mysqli_poll.md)
