---
title: "ptr_is_null() — internals"
description: "Compiler internals for ptr_is_null(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 275
---

## `ptr_is_null()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/pointers.rs`:56](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/pointers.rs#L56) (`lower_ptr_is_null`)
- **Function symbol**: `lower_ptr_is_null()`


### Lowering notes

- Lowers `ptr_is_null(pointer)` by comparing the raw pointer address to zero.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function ptr_is_null(pointer $pointer): bool
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Cross-references

- [User reference for `ptr_is_null()`](../../../php/builtins/pointer/ptr_is_null.md)

