---
title: "ptr_set() — internals"
description: "Compiler internals for ptr_set(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 298
---

## `ptr_set()` — internals

## Where it lives

- **Signature**: [`src/builtins/pointers/ptr_set.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/pointers/ptr_set.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/pointers.rs`:109](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/pointers.rs#L109) (`lower_ptr_set`)
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

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/raw_memory/ptr_set.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/raw_memory/ptr_set.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `ptr_set()`](../../../php/builtins/pointer/ptr_set.md)
