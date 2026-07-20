---
title: "ptr_sizeof() — internals"
description: "Compiler internals for ptr_sizeof(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 312
---

## `ptr_sizeof()` — internals

## Where it lives

- **Signature**: [`src/builtins/pointers/ptr_sizeof.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/pointers/ptr_sizeof.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/pointers.rs`:70](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/pointers.rs#L70) (`lower_ptr_sizeof`)
- **Function symbol**: `lower_ptr_sizeof()`


### Lowering notes

- Lowers `ptr_sizeof("type")` by materializing the checked static byte size.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function ptr_sizeof(string $type): int
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/raw_memory/ptr_sizeof.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/raw_memory/ptr_sizeof.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `ptr_sizeof()`](../../../php/builtins/pointer/ptr_sizeof.md)
