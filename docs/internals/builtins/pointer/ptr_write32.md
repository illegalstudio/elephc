---
title: "ptr_write32() — internals"
description: "Compiler internals for ptr_write32(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 285
---

## `ptr_write32()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/pointers.rs`:166](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/pointers.rs#L166) (`lower_ptr_write32`)
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

## Cross-references

- [User reference for `ptr_write32()`](../../../php/builtins/pointer/ptr_write32.md)

