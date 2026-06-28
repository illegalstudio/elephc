---
title: "array_chunk() — internals"
description: "Compiler internals for array_chunk(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 1
---

## `array_chunk()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/arrays.rs`:81](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/arrays.rs#L81) (`lower_array_chunk`)
- **Function symbol**: `lower_array_chunk()`


### Lowering notes

- Lowers `array_chunk()` by splitting an indexed array into nested indexed arrays.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function array_chunk(array $array, int $length, bool $preserve_keys): array
```

## What the type checker enforces

- **Arity**: takes exactly 3 arguments.

## Cross-references

- [User reference for `array_chunk()`](../../../php/builtins/array/array_chunk.md)

