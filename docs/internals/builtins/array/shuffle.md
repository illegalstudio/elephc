---
title: "shuffle() — internals"
description: "Compiler internals for shuffle(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 42
---

## `shuffle()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/arrays.rs`:1116](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/arrays.rs#L1116) (`lower_shuffle`)
- **Function symbol**: `lower_shuffle()`


### Lowering notes

- Lowers `shuffle()` for indexed arrays with 8-byte slots by mutating the source array in place.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function shuffle(array $array): bool
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.
- **By-reference parameters**: `$array`.

## Cross-references

- [User reference for `shuffle()`](../../../php/builtins/array/shuffle.md)

