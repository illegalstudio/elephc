---
title: "stream_bucket_new() — internals"
description: "Compiler internals for stream_bucket_new(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 173
---

## `stream_bucket_new()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/io.rs`:1970](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/io.rs#L1970) (`lower_stream_bucket_new`)
- **Function symbol**: `lower_stream_bucket_new()`


### Lowering notes

- Lowers `stream_bucket_new(stream, data)` into a stdClass-backed bucket object.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function stream_bucket_new(resource $stream, string $buffer): mixed
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Cross-references

- [User reference for `stream_bucket_new()`](../../../php/builtins/io/stream_bucket_new.md)

