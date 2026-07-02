---
title: "zval_free() — internals"
description: "Compiler internals for zval_free(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 303
---

## `zval_free()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/pointers.rs`:630](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/pointers.rs#L630) (`lower_zval_free`)
- **Function symbol**: `lower_zval_free()`


### Lowering notes

- Lowers `zval_free(zval_ptr)` by invoking `__rt_zval_free` to release the zval
- block and any PHP-shaped children it owns. The call has no result value.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_zval_free`

## Signature summary

```php
function zval_free(mixed $zval): void
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Cross-references

- [User reference for `zval_free()`](../../../php/builtins/pointer/zval_free.md)

