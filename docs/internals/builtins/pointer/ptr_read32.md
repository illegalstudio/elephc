---
title: "ptr_read32() — internals"
description: "Compiler internals for ptr_read32(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 295
---

## `ptr_read32()` — internals

## Where it lives

- **Signature**: [`src/builtins/pointers/ptr_read32.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/pointers/ptr_read32.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/pointers.rs`:124](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/pointers.rs#L124) (`lower_ptr_read32`)
- **Function symbol**: `lower_ptr_read32()`


### Lowering notes

- Lowers `ptr_read32(pointer)` by reading one unsigned 32-bit word through a checked pointer.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_ptr_read_string`

## Signature summary

```php
function ptr_read32(pointer $pointer): int
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/raw_memory/ptr_read32.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/raw_memory/ptr_read32.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `ptr_read32()`](../../../php/builtins/pointer/ptr_read32.md)
