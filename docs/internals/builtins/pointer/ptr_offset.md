---
title: "ptr_offset() — internals"
description: "Compiler internals for ptr_offset(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 306
---

## `ptr_offset()` — internals

## Where it lives

- **Signature**: [`src/builtins/pointers/ptr_offset.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/pointers/ptr_offset.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/pointers.rs`:81](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/pointers.rs#L81) (`lower_ptr_offset`)
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

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/raw_memory/ptr_offset.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/raw_memory/ptr_offset.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `ptr_offset()`](../../../php/builtins/pointer/ptr_offset.md)
