---
title: "stream_get_contents() — internals"
description: "Compiler internals for stream_get_contents(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 184
---

## `stream_get_contents()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/io.rs`:1301](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/io.rs#L1301) (`lower_stream_get_contents`)
- **Function symbol**: `lower_stream_get_contents()`


### Lowering notes

- Lowers `stream_get_contents(stream, length?, offset?)` to `string|false`.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function stream_get_contents(resource $stream, int $length, int $offset): mixed
```

## What the type checker enforces

- **Arity**: takes 1–3 arguments (2 optional).

## Cross-references

- [User reference for `stream_get_contents()`](../../../php/builtins/io/stream_get_contents.md)

