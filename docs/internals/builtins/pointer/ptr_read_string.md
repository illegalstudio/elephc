---
title: "ptr_read_string() — internals"
description: "Compiler internals for ptr_read_string(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 297
---

## `ptr_read_string()` — internals

## Where it lives

- **Signature**: [`src/builtins/pointers/ptr_read_string.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/pointers/ptr_read_string.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/pointers.rs`:288](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/pointers.rs#L288) (`lower_ptr_read_string`)
- **Function symbol**: `lower_ptr_read_string()`


### Lowering notes

- Lowers `ptr_read_string(pointer, length)` by copying raw bytes into an owned PHP string.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_ptr_read_string`

## Signature summary

```php
function ptr_read_string(pointer $pointer, int $length): string
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/raw_memory/ptr_read_string.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/raw_memory/ptr_read_string.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `ptr_read_string()`](../../../php/builtins/pointer/ptr_read_string.md)
