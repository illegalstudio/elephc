---
title: "array_keys() — internals"
description: "Compiler internals for array_keys(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 13
---

## `array_keys()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/arrays/keys.rs`:23](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/arrays/keys.rs#L23) (`lower_array_keys`)
- **Function symbol**: `lower_array_keys()`


### Lowering notes

- Lowers `array_keys()` for indexed arrays and associative arrays.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function array_keys(array $array, string $filter_value, bool $strict): array
```

## What the type checker enforces

- **Arity**: takes exactly 3 arguments.

## Cross-references

- [User reference for `array_keys()`](../../../php/builtins/array/array_keys.md)

