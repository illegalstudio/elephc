---
title: "array_pad() — internals"
description: "Compiler internals for array_pad(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 16
---

## `array_pad()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/arrays.rs`:99](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/arrays.rs#L99) (`lower_array_pad`)
- **Function symbol**: `lower_array_pad()`


### Lowering notes

- Lowers `array_pad()` by copying an indexed array and filling missing slots.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function array_pad(array $array, int $length, mixed $value): array
```

## What the type checker enforces

- **Arity**: takes exactly 3 arguments.

## Cross-references

- [User reference for `array_pad()`](../../../php/builtins/array/array_pad.md)

