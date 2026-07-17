---
title: "buffer_len() — internals"
description: "Compiler internals for buffer_len(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 65
---

## `buffer_len()` — internals

## Where it lives

- **Signature**: [`src/builtins/pointers/buffer_len.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/pointers/buffer_len.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/buffers.rs`:20](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/buffers.rs#L20) (`lower_buffer_len`)
- **Function symbol**: `lower_buffer_len()`


### Lowering notes

- Lowers `buffer_len()` through the direct buffer opcode helper.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function buffer_len(buffer $buffer): int
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/raw_memory/buffer_len.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/raw_memory/buffer_len.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `buffer_len()`](../../../php/builtins/buffer/buffer_len.md)
