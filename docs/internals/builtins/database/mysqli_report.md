---
title: "mysqli_report() — internals"
description: "Compiler internals for mysqli_report(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 151
---

## `mysqli_report()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/mysqli_prelude/build/exception.rs`:90](https://github.com/illegalstudio/elephc/blob/main/src/mysqli_prelude/build/exception.rs#L90) (`mysqli_report`)
- **Function symbol**: `mysqli_report()`


### Lowering notes

- Implemented by the compiler-injected mysqli prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function mysqli_report(int $flags): bool
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `mysqli_report()`](../../../php/builtins/database/mysqli_report.md)
