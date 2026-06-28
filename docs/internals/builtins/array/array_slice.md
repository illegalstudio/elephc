---
title: "array_slice() — internals"
description: "Compiler internals for array_slice(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 25
---

## `array_slice()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/arrays.rs`:906](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/arrays.rs#L906) (`lower_array_slice`)
- **Function symbol**: `lower_array_slice()`


### Lowering notes

- Lowers `array_slice()` for indexed arrays with pointer-sized payload slots.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function array_slice(array $array, int $offset, int $length, bool $preserve_keys): array
```

## What the type checker enforces

- **Arity**: takes 3–4 arguments (1 optional).

## Cross-references

- [User reference for `array_slice()`](../../../php/builtins/array/array_slice.md)

