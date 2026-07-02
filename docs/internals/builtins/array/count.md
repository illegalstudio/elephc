---
title: "count() — internals"
description: "Compiler internals for count(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 51
---

## `count()` — internals

## Where it lives

- **Signature**: [`src/builtins/array/count.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/array/count.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins.rs`:440](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins.rs#L440) (`lower_count`)
- **Function symbol**: `lower_count()`


### Lowering notes

- Lowers `count(array)` for concrete array values by reading the runtime length header.
- Called from `crate::builtins::array::count` (the registry home) via a thin wrapper.
- Handles Array/AssocArray (reads length directly from the runtime header), Mixed/Union
- (delegates to `__rt_mixed_count`), and Countable Object (calls the object's `count`
- method via intrinsic or dynamic dispatch).

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_mixed_count`

## Signature summary

```php
function count(array $value, int $mode = 0): int
```

## What the type checker enforces

- **Arity**: takes 1–2 arguments (1 optional).

## Cross-references

- [User reference for `count()`](../../../php/builtins/array/count.md)

