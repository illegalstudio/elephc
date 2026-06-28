---
title: "stream_context_set_params() — internals"
description: "Compiler internals for stream_context_set_params(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 180
---

## `stream_context_set_params()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/io.rs`:1005](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/io.rs#L1005) (`lower_stream_context_set_params`)
- **Function symbol**: `lower_stream_context_set_params()`


### Lowering notes

- Lowers `stream_context_set_params(context, params)` as an accepted parameter update.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function stream_context_set_params(resource $context, array $params): bool
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Cross-references

- [User reference for `stream_context_set_params()`](../../../php/builtins/io/stream_context_set_params.md)

