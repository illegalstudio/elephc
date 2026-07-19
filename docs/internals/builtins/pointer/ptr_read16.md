---
title: "ptr_read16() — internals"
description: "Compiler internals for ptr_read16(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 294
---

## `ptr_read16()` — internals

## Where it lives

- **Signature**: [`src/builtins/pointers/ptr_read16.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/pointers/ptr_read16.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/pointers.rs`:278](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/pointers.rs#L278) (`lower_ptr_read16`)
- **Function symbol**: `lower_ptr_read16()`


### Lowering notes

- Lowers `ptr_read16(pointer)` by reading one unsigned 16-bit word through a checked pointer.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_ptr_read_string`

## Signature summary

```php
function ptr_read16(pointer $pointer): int
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/raw_memory/ptr_read16.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/raw_memory/ptr_read16.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `ptr_read16()`](../../../php/builtins/pointer/ptr_read16.md)
