---
title: "stream_set_timeout() — internals"
description: "Compiler internals for stream_set_timeout(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 197
---

## `stream_set_timeout()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/io.rs`:2267](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/io.rs#L2267) (`lower_stream_set_timeout`)
- **Function symbol**: `lower_stream_set_timeout()`


### Lowering notes

- Lowers `stream_set_timeout(stream, seconds, microseconds?)`.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function stream_set_timeout(resource $stream, int $seconds, int $microseconds): bool
```

## What the type checker enforces

- **Arity**: takes 2–3 arguments (1 optional).

## Cross-references

- [User reference for `stream_set_timeout()`](../../../php/builtins/io/stream_set_timeout.md)

