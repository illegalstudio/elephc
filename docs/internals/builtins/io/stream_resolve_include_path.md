---
title: "stream_resolve_include_path() — internals"
description: "Compiler internals for stream_resolve_include_path(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 192
---

## `stream_resolve_include_path()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/io.rs`:2142](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/io.rs#L2142) (`lower_stream_resolve_include_path`)
- **Function symbol**: `lower_stream_resolve_include_path()`


### Lowering notes

- Lowers `stream_resolve_include_path(filename)` as realpath-backed `string|false`.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_realpath`
- `__rt_stream_socket_server`

## Signature summary

```php
function stream_resolve_include_path(string $filename): mixed
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Cross-references

- [User reference for `stream_resolve_include_path()`](../../../php/builtins/io/stream_resolve_include_path.md)

