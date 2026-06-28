---
title: "array_push() — internals"
description: "Compiler internals for array_push(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 19
---

## `array_push()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/arrays.rs`:61](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/arrays.rs#L61) (`lower_array_push`)
- **Function symbol**: `lower_array_push()`


### Lowering notes

- Lowers `array_push()` by appending one value and publishing the mutated array.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function array_push(array $array, ...$values): void
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.
- **By-reference parameters**: `$array`.
- **Variadic**: collects excess arguments into `$values`.

## Cross-references

- [User reference for `array_push()`](../../../php/builtins/array/array_push.md)

