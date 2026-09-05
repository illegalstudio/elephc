---
title: "mysqli_more_results() — internals"
description: "Compiler internals for mysqli_more_results(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 138
---

## `mysqli_more_results()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/mysqli_prelude/build/procedural.rs`:855](https://github.com/illegalstudio/elephc/blob/main/src/mysqli_prelude/build/procedural.rs#L855) (`mysqli_more_results`)
- **Function symbol**: `mysqli_more_results()`


### Lowering notes

- Implemented by the compiler-injected mysqli prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function mysqli_more_results(mixed $mysql): bool
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `mysqli_more_results()`](../../../php/builtins/database/mysqli_more_results.md)
