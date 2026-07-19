---
title: "stream_get_contents() — internals"
description: "Compiler internals for stream_get_contents(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 218
---

## `stream_get_contents()` — internals

## Where it lives

- **Signature**: [`src/builtins/io/stream_get_contents.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/io/stream_get_contents.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/io.rs`:1300](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/io.rs#L1300) (`lower_stream_get_contents`)
- **Function symbol**: `lower_stream_get_contents()`


### Lowering notes

- Lowers `stream_get_contents(stream, length?, offset?)` to `string|false`.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function stream_get_contents(resource $stream, int $length = null, int $offset = -1): mixed
```

## What the type checker enforces

- **Arity**: takes 1–3 arguments (2 optional).

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/filesystem/stream_get_contents.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/stream_get_contents.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `stream_get_contents()`](../../../php/builtins/io/stream_get_contents.md)
