---
title: "stream_get_meta_data() — internals"
description: "Compiler internals for stream_get_meta_data(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 202
---

## `stream_get_meta_data()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/io.rs`:1446](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/io.rs#L1446) (`lower_stream_get_meta_data`)
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

## Cross-references

- [User reference for `stream_get_meta_data()`](../../../php/builtins/io/stream_get_meta_data.md)

