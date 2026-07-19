---
title: "stream_set_blocking() — internals"
description: "Compiler internals for stream_set_blocking(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 228
---

## `stream_set_blocking()` — internals

## Where it lives

- **Signature**: [`src/builtins/io/stream_set_blocking.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/io/stream_set_blocking.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/io.rs`:2140](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/io.rs#L2140) (`lower_stream_set_blocking`)
- **Function symbol**: `lower_stream_set_blocking()`


### Lowering notes

- Lowers `stream_set_blocking(stream, enable)`.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_stream_set_blocking`
- `__rt_user_wrapper_set_option`

## Signature summary

```php
function stream_set_blocking(resource $stream, bool $enable): bool
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/filesystem/stream_set_blocking.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/stream_set_blocking.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `stream_set_blocking()`](../../../php/builtins/io/stream_set_blocking.md)
