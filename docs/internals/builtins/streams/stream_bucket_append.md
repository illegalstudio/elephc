---
title: "stream_bucket_append() — internals"
description: "Compiler internals for stream_bucket_append(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 330
---

## `stream_bucket_append()` — internals

## Where it lives

- **Signature**: [`src/builtins/io/stream_bucket_append.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/io/stream_bucket_append.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/io.rs`:2064](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/io.rs#L2064) (`lower_stream_bucket_append_or_prepend`)
- **Function symbol**: `lower_stream_bucket_append_or_prepend()`


### Lowering notes

- Lowers `stream_bucket_append` and `stream_bucket_prepend` over the `_buckets` array.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function stream_bucket_append(mixed $brigade, mixed $bucket): void
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Cross-references

- [User reference for `stream_bucket_append()`](../../../php/builtins/streams/stream_bucket_append.md)
