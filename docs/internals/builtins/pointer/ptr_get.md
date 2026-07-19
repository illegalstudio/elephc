---
title: "ptr_get() — internals"
description: "Compiler internals for ptr_get(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 290
---

## `ptr_get()` — internals

## Where it lives

- **Signature**: [`src/builtins/pointers/ptr_get.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/pointers/ptr_get.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/pointers.rs`:104](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/pointers.rs#L104) (`lower_ptr_get`)
- **Function symbol**: `lower_ptr_get()`


### Lowering notes

- Lowers `ptr_get(pointer)` by reading one machine word through a checked pointer.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function ptr_get(pointer $pointer): int
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/raw_memory/ptr_get.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/raw_memory/ptr_get.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `ptr_get()`](../../../php/builtins/pointer/ptr_get.md)
