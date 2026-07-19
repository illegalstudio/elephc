---
title: "ptr_write32() — internals"
description: "Compiler internals for ptr_write32(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 314
---

## `ptr_write32()` — internals

## Where it lives

- **Signature**: [`src/builtins/pointers/ptr_write32.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/pointers/ptr_write32.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/pointers.rs`:161](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/pointers.rs#L161) (`lower_ptr_write32`)
- **Function symbol**: `lower_ptr_write32()`


### Lowering notes

- Lowers `ptr_write32(pointer, value)` by writing one 32-bit word through a checked pointer.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_ptr_write_string`

## Signature summary

```php
function ptr_write32(pointer $pointer, int $value): void
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/raw_memory/ptr_write32.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/raw_memory/ptr_write32.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `ptr_write32()`](../../../php/builtins/pointer/ptr_write32.md)
