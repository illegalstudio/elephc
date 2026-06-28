---
title: "stream_bucket_prepend() — internals"
description: "Compiler internals for stream_bucket_prepend(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 317
---

## `stream_bucket_prepend()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`(not lowered)`:0]()
- **Function symbol**: `(none — type-checker only)()`


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

