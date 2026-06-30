---
title: "ptr_write_string() — internals"
description: "Compiler internals for ptr_write_string(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 302
---

## `ptr_write_string()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/pointers.rs`:171](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/pointers.rs#L171) (`lower_ptr_write_string`)
- **Function symbol**: `lower_ptr_write_string()`


### Lowering notes

- Lowers `ptr_write_string(pointer, string)` by copying PHP string bytes into raw memory.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_ptr_write_string`

## Signature summary

```php
function ptr_write_string(pointer $pointer, string $string): int
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Cross-references

- [User reference for `ptr_write_string()`](../../../php/builtins/pointer/ptr_write_string.md)

