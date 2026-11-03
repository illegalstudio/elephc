---
title: "mysqli_refresh() - internals"
description: "Compiler internals for mysqli_refresh(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 165
---

## `mysqli_refresh()` - internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/mysqli_prelude/build/procedural.rs`:510](https://github.com/illegalstudio/elephc/blob/main/src/mysqli_prelude/build/procedural.rs#L510) (`mysqli_refresh`)
- **Function symbol**: `mysqli_refresh()`


### Lowering notes

- Implemented by the compiler-injected mysqli prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function mysqli_refresh(mixed $mysql, int $flags): bool
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Eval interpreter (magician)

_Not callable from eval'd code - the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `mysqli_refresh()`](../../../php/builtins/database/mysqli_refresh.md)
