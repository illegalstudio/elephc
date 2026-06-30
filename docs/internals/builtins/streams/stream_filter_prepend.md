---
title: "stream_filter_prepend() — internals"
description: "Compiler internals for stream_filter_prepend(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 336
---

## `stream_filter_prepend()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`(not lowered)`:0]()
- **Function symbol**: `(none — type-checker only)()`


## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function stream_filter_prepend(resource $stream, string $filter_name, int $mode, mixed $params): mixed
```

## What the type checker enforces

- **Arity**: takes 2–4 arguments (2 optional).

## Cross-references

- [User reference for `stream_filter_prepend()`](../../../php/builtins/streams/stream_filter_prepend.md)

