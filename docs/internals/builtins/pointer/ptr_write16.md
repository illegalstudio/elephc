---
title: "ptr_write16() — internals"
description: "Compiler internals for ptr_write16(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 284
---

## `ptr_write16()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/pointers.rs`:161](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/pointers.rs#L161) (`lower_ptr_write16`)
- **Function symbol**: `lower_ptr_write16()`


### Lowering notes

- Lowers `ptr_write16(pointer, value)` by writing one 16-bit word through a checked pointer.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_ptr_write_string`

## Signature summary

```php
function ptr_write16(pointer $pointer, int $value): void
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Cross-references

- [User reference for `ptr_write16()`](../../../php/builtins/pointer/ptr_write16.md)

