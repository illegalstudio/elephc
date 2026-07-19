---
title: "stream_get_line() — internals"
description: "Compiler internals for stream_get_line(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 220
---

## `stream_get_line()` — internals

## Where it lives

- **Signature**: [`src/builtins/io/stream_get_line.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/io/stream_get_line.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/io.rs`:1392](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/io.rs#L1392) (`lower_stream_get_line`)
- **Function symbol**: `lower_stream_get_line()`


### Lowering notes

- Lowers `stream_get_line(stream, length, ending?)`.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function stream_get_line(resource $stream, int $length, string $ending = ''): string
```

## What the type checker enforces

- **Arity**: takes 2–3 arguments (1 optional).

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/filesystem/stream_get_line.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/stream_get_line.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `stream_get_line()`](../../../php/builtins/io/stream_get_line.md)
