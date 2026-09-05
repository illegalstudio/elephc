---
title: "mysqli_set_opt() — internals"
description: "Compiler internals for mysqli_set_opt(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 156
---

## `mysqli_set_opt()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/mysqli_prelude/build/procedural.rs`:414](https://github.com/illegalstudio/elephc/blob/main/src/mysqli_prelude/build/procedural.rs#L414) (`mysqli_set_opt`)
- **Function symbol**: `mysqli_set_opt()`


### Lowering notes

- Implemented by the compiler-injected mysqli prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function mysqli_set_opt(mixed $mysql, int $option, mixed $value): bool
```

## What the type checker enforces

- **Arity**: takes exactly 3 arguments.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `mysqli_set_opt()`](../../../php/builtins/database/mysqli_set_opt.md)
