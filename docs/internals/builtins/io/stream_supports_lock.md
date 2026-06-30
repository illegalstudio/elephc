---
title: "stream_supports_lock() — internals"
description: "Compiler internals for stream_supports_lock(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 223
---

## `stream_supports_lock()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/io.rs`:2115](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/io.rs#L2115) (`lower_stream_supports_lock`)
- **Function symbol**: `lower_stream_supports_lock()`


### Lowering notes

- Lowers `stream_supports_lock(stream)` as true after resource unboxing.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_stream_isatty`

## Signature summary

```php
function stream_supports_lock(resource $stream): bool
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Cross-references

- [User reference for `stream_supports_lock()`](../../../php/builtins/io/stream_supports_lock.md)

