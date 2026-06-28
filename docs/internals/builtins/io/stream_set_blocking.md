---
title: "stream_set_blocking() — internals"
description: "Compiler internals for stream_set_blocking(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 194
---

## `stream_set_blocking()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/io.rs`:1923](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/io.rs#L1923) (`lower_stream_set_blocking`)
- **Function symbol**: `lower_stream_set_blocking()`


### Lowering notes

- Lowers `stream_set_blocking(stream, enable)`.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_stream_set_blocking`
- `__rt_user_wrapper_set_option`

## Signature summary

```php
function stream_set_blocking(resource $stream, bool $enable): bool
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Cross-references

- [User reference for `stream_set_blocking()`](../../../php/builtins/io/stream_set_blocking.md)

