---
title: "mysqli_options() — internals"
description: "Compiler internals for mysqli_options(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 143
---

## `mysqli_options()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/mysqli_prelude/build/procedural.rs`:393](https://github.com/illegalstudio/elephc/blob/main/src/mysqli_prelude/build/procedural.rs#L393) (`mysqli_options`)
- **Function symbol**: `mysqli_options()`


### Lowering notes

- Implemented by the compiler-injected mysqli prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function mysqli_options(mixed $mysql, int $option, mixed $value): bool
```

## What the type checker enforces

- **Arity**: takes exactly 3 arguments.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `mysqli_options()`](../../../php/builtins/database/mysqli_options.md)
