---
title: "stream_filter_append() — internals"
description: "Compiler internals for stream_filter_append(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 354
---

## `stream_filter_append()` — internals

## Where it lives

- **Signature**: [`src/builtins/io/stream_filter_append.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/io/stream_filter_append.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/io.rs`:1548](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/io.rs#L1548) (`lower_stream_filter_attach`)
- **Function symbol**: `lower_stream_filter_attach()`


### Lowering notes

- Lowers `stream_filter_append` and `stream_filter_prepend`.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function stream_filter_append(resource $stream, string $filtername, int $read_write = 3, mixed $params = null): mixed
```

## What the type checker enforces

- **Arity**: takes 2–4 arguments (2 optional).

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/filesystem/stream_filter_append.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/stream_filter_append.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `stream_filter_append()`](../../../php/builtins/streams/stream_filter_append.md)
