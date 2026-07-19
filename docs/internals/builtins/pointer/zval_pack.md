---
title: "zval_pack() — internals"
description: "Compiler internals for zval_pack(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 305
---

## `zval_pack()` — internals

## Where it lives

- **Signature**: [`src/builtins/pointers/zval_pack.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/pointers/zval_pack.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/pointers.rs`:724](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/pointers.rs#L724) (`lower_zval_pack`)
- **Function symbol**: `lower_zval_pack()`


### Lowering notes

- Lowers `zval_pack(value)` by boxing the operand as a Mixed cell and invoking
- `__rt_zval_pack`, which returns a pointer to a freshly allocated 16-byte zval.
- `__rt_zval_pack` only reads the `(tag, lo, hi)` triple out of the boxed Mixed
- cell; it never retains or frees that cell. When the operand was not already
- Mixed/Union, `emit_box_current_value_as_mixed` allocated a fresh owned box
- (persisting strings, increfing array/object/mixed children), so that box is a
- throwaway temporary that must be deep-released after the pack call or it leaks
- one Mixed cell per call. When the operand is already Mixed/Union no box was
- created, so the operand's own live cell must not be freed here.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_mixed_free_deep`
- `__rt_zval_pack`

## Signature summary

```php
function zval_pack(mixed $value): pointer
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `zval_pack()`](../../../php/builtins/pointer/zval_pack.md)
