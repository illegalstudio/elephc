---
title: "ptr_write8() — internals"
description: "Compiler internals for ptr_write8(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 302
---

## `ptr_write8()` — internals

## Where it lives

- **Signature**: [`src/builtins/pointers/ptr_write8.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/pointers/ptr_write8.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/pointers.rs`:156](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/pointers.rs#L156) (`lower_ptr_write8`)
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

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/raw_memory/ptr_write8.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/raw_memory/ptr_write8.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `ptr_write8()`](../../../php/builtins/pointer/ptr_write8.md)
