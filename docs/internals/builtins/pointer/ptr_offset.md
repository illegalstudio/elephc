---
title: "ptr_offset() — internals"
description: "Compiler internals for ptr_offset(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 277
---

## `ptr_offset()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/pointers.rs`:86](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/pointers.rs#L86) (`lower_ptr_offset`)
- **Function symbol**: `lower_ptr_offset()`


### Lowering notes

- Lowers `ptr_offset(pointer, offset)` by adding a byte offset to a raw address.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function ptr_offset(pointer $pointer, int $offset): mixed
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Cross-references

- [User reference for `ptr_offset()`](../../../php/builtins/pointer/ptr_offset.md)

