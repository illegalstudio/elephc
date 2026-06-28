---
title: "in_array() — internals"
description: "Compiler internals for in_array(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 35
---

## `in_array()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/arrays.rs`:1160](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/arrays.rs#L1160) (`lower_in_array`)
- **Function symbol**: `lower_in_array()`


### Lowering notes

- Lowers `in_array()` for indexed arrays with scalar or string payloads.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function in_array(mixed $needle, array $haystack, bool $strict): mixed
```

## What the type checker enforces

- **Arity**: takes 2–3 arguments (1 optional).

## Cross-references

- [User reference for `in_array()`](../../../php/builtins/array/in_array.md)

