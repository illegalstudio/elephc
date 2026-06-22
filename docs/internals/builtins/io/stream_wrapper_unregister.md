---
title: "stream_wrapper_unregister() — internals"
description: "Compiler internals for stream_wrapper_unregister(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 211
---

## `stream_wrapper_unregister()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/io.rs`:1030](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/io.rs#L1030) (`lower_stream_wrapper_unregister`)
- **Function symbol**: `lower_stream_wrapper_unregister()`


### Lowering notes

- Lowers `stream_wrapper_unregister(protocol)`.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_stream_wrapper_unregister`

## Signature summary

```php
function stream_wrapper_unregister(string $protocol): bool
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Cross-references

- [User reference for `stream_wrapper_unregister()`](../../../php/builtins/io/stream_wrapper_unregister.md)

