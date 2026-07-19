---
title: "stream_bucket_make_writeable() — internals"
description: "Compiler internals for stream_bucket_make_writeable(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 193
---

## `stream_bucket_make_writeable()` — internals

## Where it lives

- **Signature**: [`src/builtins/io/stream_bucket_make_writeable.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/io/stream_bucket_make_writeable.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/io.rs`:1987](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/io.rs#L1987) (`lower_stream_bucket_make_writeable`)
- **Function symbol**: `lower_stream_bucket_make_writeable()`


### Lowering notes

- Lowers `stream_bucket_make_writeable(brigade)` by popping the brigade head.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_stdclass_get`

## Signature summary

```php
function stream_bucket_make_writeable(mixed $brigade): mixed
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/filesystem/stream_bucket_make_writeable.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/stream_bucket_make_writeable.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `stream_bucket_make_writeable()`](../../../php/builtins/io/stream_bucket_make_writeable.md)
