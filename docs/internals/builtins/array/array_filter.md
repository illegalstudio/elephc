---
title: "array_filter() — internals"
description: "Compiler internals for array_filter(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 11
---

## `array_filter()` — internals

## Where it lives

- **Signature**: [`src/builtins/array/array_filter.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/array/array_filter.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/arrays.rs`:217](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/arrays.rs#L217) (`lower_array_filter`)
- **Function symbol**: `lower_array_filter()`


### Lowering notes

- Lowers `array_filter()` for static and first-class callbacks through the runtime helper.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_array_filter`
- `__rt_array_filter_refcounted`

## Signature summary

```php
function array_filter(array $array, callable $callback = null, int $mode = 0): array
```

## What the type checker enforces

- **Arity**: takes 1–3 arguments (2 optional).

## Cross-references

- [User reference for `array_filter()`](../../../php/builtins/array/array_filter.md)
