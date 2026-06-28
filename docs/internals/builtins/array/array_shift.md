---
title: "array_shift() — internals"
description: "Compiler internals for array_shift(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 24
---

## `array_shift()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/arrays/shift.rs`:23](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/arrays/shift.rs#L23) (`lower_array_shift`)
- **Function symbol**: `lower_array_shift()`


### Lowering notes

- Lowers `array_shift()` for indexed arrays by compacting slots and boxing `T|null` as Mixed.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function array_shift(array $array): mixed
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.
- **By-reference parameters**: `$array`.

## Cross-references

- [User reference for `array_shift()`](../../../php/builtins/array/array_shift.md)

