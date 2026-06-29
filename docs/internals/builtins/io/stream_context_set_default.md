---
title: "stream_context_set_default() — internals"
description: "Compiler internals for stream_context_set_default(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 178
---

## `stream_context_set_default()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/io.rs`:1088](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/io.rs#L1088) (`lower_stream_context_set_default`)
- **Function symbol**: `lower_stream_context_set_default()`


### Lowering notes

- Lowers `stream_context_set_default(options)`.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function stream_context_set_default(array $options): mixed
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Cross-references

- [User reference for `stream_context_set_default()`](../../../php/builtins/io/stream_context_set_default.md)

