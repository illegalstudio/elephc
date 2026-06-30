---
title: "ptr_read16() — internals"
description: "Compiler internals for ptr_read16(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 293
---

## `ptr_read16()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/pointers.rs`:124](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/pointers.rs#L124) (`lower_ptr_read16`)
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

## Cross-references

- [User reference for `ptr_read16()`](../../../php/builtins/pointer/ptr_read16.md)

