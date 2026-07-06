---
title: "stream_bucket_prepend() — internals"
description: "Compiler internals for stream_bucket_prepend(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 331
---

## `stream_bucket_prepend()` — internals

## Where it lives

- **Signature**: [`src/builtins/io/stream_bucket_prepend.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/io/stream_bucket_prepend.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/io.rs`:2064](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/io.rs#L2064) (`lower_stream_bucket_append_or_prepend`)
- **Function symbol**: `lower_stream_bucket_append_or_prepend()`


### Lowering notes

- Lowers `stream_bucket_append` and `stream_bucket_prepend` over the `_buckets` array.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function stream_bucket_prepend(mixed $brigade, mixed $bucket): void
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Cross-references

- [User reference for `stream_bucket_prepend()`](../../../php/builtins/streams/stream_bucket_prepend.md)
