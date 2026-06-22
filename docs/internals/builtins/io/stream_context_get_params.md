---
title: "stream_context_get_params() — internals"
description: "Compiler internals for stream_context_get_params(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 177
---

## `stream_context_get_params()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/io.rs`:1291](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/io.rs#L1291) (`lower_stream_context_get_params`)
- **Function symbol**: `lower_stream_context_get_params()`


### Lowering notes

- Lowers `stream_context_get_params(context)` to an empty associative hash.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function stream_context_get_params(resource $context): array
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Cross-references

- [User reference for `stream_context_get_params()`](../../../php/builtins/io/stream_context_get_params.md)

