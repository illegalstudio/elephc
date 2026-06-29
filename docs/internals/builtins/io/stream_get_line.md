---
title: "stream_get_line() — internals"
description: "Compiler internals for stream_get_line(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 186
---

## `stream_get_line()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/io.rs`:1393](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/io.rs#L1393) (`lower_stream_get_line`)
- **Function symbol**: `lower_stream_get_line()`


### Lowering notes

- Lowers `stream_get_line(stream, length, ending?)`.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function stream_get_line(resource $stream, int $length, string $ending): string
```

## What the type checker enforces

- **Arity**: takes 2–3 arguments (1 optional).

## Cross-references

- [User reference for `stream_get_line()`](../../../php/builtins/io/stream_get_line.md)

