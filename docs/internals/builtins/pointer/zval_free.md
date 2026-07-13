---
title: "zval_free() — internals"
description: "Compiler internals for zval_free(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 304
---

## `zval_free()` — internals

## Where it lives

- **Signature**: [`src/builtins/pointers/zval_free.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/pointers/zval_free.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/pointers.rs`:606](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/pointers.rs#L606) (`lower_zval_free`)
- **Function symbol**: `lower_zval_free()`


### Lowering notes

- Lowers `zval_free(zval_ptr)` by releasing the zval block and owned children.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_zval_free`

## Signature summary

```php
function zval_free(pointer $zval): void
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `zval_free()`](../../../php/builtins/pointer/zval_free.md)
