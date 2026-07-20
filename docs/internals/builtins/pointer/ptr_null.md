---
title: "ptr_null() — internals"
description: "Compiler internals for ptr_null(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 305
---

## `ptr_null()` — internals

## Where it lives

- **Signature**: [`src/builtins/pointers/ptr_null.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/pointers/ptr_null.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/pointers.rs`:44](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/pointers.rs#L44) (`lower_ptr_null`)
- **Function symbol**: `lower_ptr_null()`


### Lowering notes

- Lowers `ptr_null()` by materializing the raw null pointer sentinel.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function ptr_null(): mixed
```

## What the type checker enforces

- **Arity**: takes no arguments.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/raw_memory/ptr_null.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/raw_memory/ptr_null.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `ptr_null()`](../../../php/builtins/pointer/ptr_null.md)
