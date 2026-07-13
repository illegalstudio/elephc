---
title: "zval_type() — internals"
description: "Compiler internals for zval_type(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 306
---

## `zval_type()` — internals

## Where it lives

- **Signature**: [`src/builtins/pointers/zval_type.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/pointers/zval_type.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/pointers.rs`:597](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/pointers.rs#L597) (`lower_zval_type`)
- **Function symbol**: `lower_zval_type()`


### Lowering notes

- Lowers `zval_type(zval_ptr)` by returning the PHP `IS_*` type byte.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_zval_free`
- `__rt_zval_type`

## Signature summary

```php
function zval_type(pointer $zval): int
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `zval_type()`](../../../php/builtins/pointer/zval_type.md)
