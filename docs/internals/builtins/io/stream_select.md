---
title: "stream_select() — internals"
description: "Compiler internals for stream_select(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 227
---

## `stream_select()` — internals

## Where it lives

- **Signature**: [`src/builtins/io/stream_select.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/io/stream_select.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/io.rs`:2315](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/io.rs#L2315) (`lower_stream_select`)
- **Function symbol**: `lower_stream_select()`


### Lowering notes

- Lowers `stream_select(read, write, except, seconds, microseconds?)`.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function stream_select(array $read, array $write, array $except, int $seconds, int $microseconds = 0): int
```

## What the type checker enforces

- **Arity**: takes 4–5 arguments (1 optional).
- **By-reference parameters**: `$read`, `$write`, `$except`.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/filesystem/stream_select.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/stream_select.rs) (`eval_builtin!`)
- **Dispatch hooks**: `values`
- **By-reference parameters**: `$read`, `$write`, `$except`.

## Cross-references

- [User reference for `stream_select()`](../../../php/builtins/io/stream_select.md)
