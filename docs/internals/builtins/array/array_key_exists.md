---
title: "array_key_exists() — internals"
description: "Compiler internals for array_key_exists(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 12
---

## `array_key_exists()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/arrays/key_exists.rs`:22](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/arrays/key_exists.rs#L22) (`lower_array_key_exists`)
- **Function symbol**: `lower_array_key_exists()`


### Lowering notes

- Lowers `array_key_exists()` for indexed arrays and associative arrays.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function array_key_exists(string $key, array $array): bool
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Cross-references

- [User reference for `array_key_exists()`](../../../php/builtins/array/array_key_exists.md)

