---
title: "stream_isatty() — internals"
description: "Compiler internals for stream_isatty(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 208
---

## `stream_isatty()` — internals

## Where it lives

- **Signature**: [`src/builtins/io/stream_isatty.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/io/stream_isatty.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/io.rs`:2127](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/io.rs#L2127) (`lower_stream_isatty`)
- **Function symbol**: `lower_stream_isatty()`


### Lowering notes

- Lowers `stream_isatty(stream)`.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_stream_isatty`

## Signature summary

```php
function stream_isatty(resource $stream): bool
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Cross-references

- [User reference for `stream_isatty()`](../../../php/builtins/io/stream_isatty.md)
