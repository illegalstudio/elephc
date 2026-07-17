---
title: "buffer_free() — internals"
description: "Compiler internals for buffer_free(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 64
---

## `buffer_free()` — internals

## Where it lives

- **Signature**: [`src/builtins/pointers/buffer_free.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/pointers/buffer_free.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/buffers.rs`:25](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/buffers.rs#L25) (`lower_buffer_free`)
- **Function symbol**: `lower_buffer_free()`


### Lowering notes

- Lowers `buffer_free()` through the direct buffer opcode helper.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function buffer_free(buffer $buffer): void
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/raw_memory/buffer_free.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/raw_memory/buffer_free.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `buffer_free()`](../../../php/builtins/buffer/buffer_free.md)
