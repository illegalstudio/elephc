---
title: "stream_context_get_options() — internals"
description: "Compiler internals for stream_context_get_options(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 191
---

## `stream_context_get_options()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/io.rs`:1252](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/io.rs#L1252) (`lower_stream_context_get_options`)
- **Function symbol**: `lower_stream_context_get_options()`


### Lowering notes

- Lowers `stream_context_get_options(context)`.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_hash_new`
- `__rt_incref`

## Signature summary

```php
function stream_context_get_options(resource $stream_or_context): array
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Cross-references

- [User reference for `stream_context_get_options()`](../../../php/builtins/io/stream_context_get_options.md)

