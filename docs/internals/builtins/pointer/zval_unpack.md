---
title: "zval_unpack() — internals"
description: "Compiler internals for zval_unpack(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 303
---

## `zval_unpack()` — internals

## Where it lives

- **Signature**: [`src/builtins/pointers/zval_unpack.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/pointers/zval_unpack.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/pointers.rs`:588](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/pointers.rs#L588) (`lower_zval_unpack`)
- **Function symbol**: `lower_zval_unpack()`


### Lowering notes

- Lowers `zval_unpack(zval_ptr)` by invoking `__rt_zval_unpack`.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_zval_free`
- `__rt_zval_type`
- `__rt_zval_unpack`

## Signature summary

```php
function zval_unpack(pointer $zval): mixed
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Cross-references

- [User reference for `zval_unpack()`](../../../php/builtins/pointer/zval_unpack.md)
