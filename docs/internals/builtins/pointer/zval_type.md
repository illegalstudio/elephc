---
title: "zval_type() — internals"
description: "Compiler internals for zval_type(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 305
---

## `zval_type()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/pointers.rs`:620](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/pointers.rs#L620) (`lower_zval_type`)
- **Function symbol**: `lower_zval_type()`


### Lowering notes

- Lowers `zval_type(zval_ptr)` by invoking `__rt_zval_type`, which returns the
- PHP `IS_*` type byte as an integer.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_zval_free`
- `__rt_zval_type`

## Signature summary

```php
function zval_type(mixed $zval): int
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Cross-references

- [User reference for `zval_type()`](../../../php/builtins/pointer/zval_type.md)

