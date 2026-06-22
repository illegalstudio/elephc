---
title: "stream_context_get_default() — internals"
description: "Compiler internals for stream_context_get_default(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 175
---

## `stream_context_get_default()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/io.rs`:1078](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/io.rs#L1078) (`lower_stream_context_get_default`)
- **Function symbol**: `lower_stream_context_get_default()`


### Lowering notes

- Lowers `stream_context_get_default(options?)`.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function stream_context_get_default(array $options): mixed
```

## What the type checker enforces

- **Arity**: takes 0–1 arguments (1 optional).

## Cross-references

- [User reference for `stream_context_get_default()`](../../../php/builtins/io/stream_context_get_default.md)

