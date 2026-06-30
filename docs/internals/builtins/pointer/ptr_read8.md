---
title: "ptr_read8() — internals"
description: "Compiler internals for ptr_read8(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 295
---

## `ptr_read8()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/pointers.rs`:119](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/pointers.rs#L119) (`lower_ptr_read8`)
- **Function symbol**: `lower_ptr_read8()`


### Lowering notes

- Lowers `ptr_read8(pointer)` by reading one unsigned byte through a checked pointer.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function ptr_read8(pointer $pointer): int
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Cross-references

- [User reference for `ptr_read8()`](../../../php/builtins/pointer/ptr_read8.md)

