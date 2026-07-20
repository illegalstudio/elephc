---
title: "stream_get_meta_data() — internals"
description: "Compiler internals for stream_get_meta_data(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 221
---

## `stream_get_meta_data()` — internals

## Where it lives

- **Signature**: [`src/builtins/io/stream_get_meta_data.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/io/stream_get_meta_data.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/io.rs`:1444](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/io.rs#L1444) (`lower_stream_get_meta_data`)
- **Function symbol**: `lower_stream_get_meta_data()`


### Lowering notes

- Lowers `stream_get_meta_data(stream)` through the metadata runtime helper.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_stream_get_meta_data`

## Signature summary

```php
function stream_get_meta_data(resource $stream): array
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/filesystem/stream_get_meta_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/stream_get_meta_data.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `stream_get_meta_data()`](../../../php/builtins/io/stream_get_meta_data.md)
