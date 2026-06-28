---
title: "buffer_free() — internals"
description: "Compiler internals for buffer_free(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 47
---

## `buffer_free()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/buffers.rs`:24](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/buffers.rs#L24) (`lower_buffer_free`)
- **Function symbol**: `lower_buffer_free()`


### Lowering notes

- Lowers `buffer_free()` through the direct buffer opcode helper.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function buffer_free(buffer $buffer): mixed
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Cross-references

- [User reference for `buffer_free()`](../../../php/builtins/buffer/buffer_free.md)

