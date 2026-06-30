---
title: "stream_copy_to_stream() — internals"
description: "Compiler internals for stream_copy_to_stream(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 196
---

## `stream_copy_to_stream()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/io.rs`:1360](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/io.rs#L1360) (`lower_stream_copy_to_stream`)
- **Function symbol**: `lower_stream_copy_to_stream()`


### Lowering notes

- Lowers `stream_copy_to_stream(from, to, length?, offset?)` through wrapper-aware read/write loops.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function stream_copy_to_stream(resource $from, resource $to, int $length, int $offset): mixed
```

## What the type checker enforces

- **Arity**: takes 2–4 arguments (2 optional).

## Cross-references

- [User reference for `stream_copy_to_stream()`](../../../php/builtins/io/stream_copy_to_stream.md)

