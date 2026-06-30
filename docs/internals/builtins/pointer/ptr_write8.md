---
title: "ptr_write8() — internals"
description: "Compiler internals for ptr_write8(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 301
---

## `ptr_write8()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/pointers.rs`:156](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/pointers.rs#L156) (`lower_ptr_write8`)
- **Function symbol**: `lower_ptr_write8()`


### Lowering notes

- Lowers `ptr_write8(pointer, value)` by writing one byte through a checked pointer.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function ptr_write8(pointer $pointer, int $value): void
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Cross-references

- [User reference for `ptr_write8()`](../../../php/builtins/pointer/ptr_write8.md)

