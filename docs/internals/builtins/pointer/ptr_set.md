---
title: "ptr_set() — internals"
description: "Compiler internals for ptr_set(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 297
---

## `ptr_set()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/pointers.rs`:114](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/pointers.rs#L114) (`lower_ptr_set`)
- **Function symbol**: `lower_ptr_set()`


### Lowering notes

- Lowers `ptr_set(pointer, value)` by writing one machine word through a checked pointer.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function ptr_set(pointer $pointer, mixed $value): void
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Cross-references

- [User reference for `ptr_set()`](../../../php/builtins/pointer/ptr_set.md)

