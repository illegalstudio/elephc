---
title: "stream_filter_register() — internals"
description: "Compiler internals for stream_filter_register(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 197
---

## `stream_filter_register()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/io.rs`:1521](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/io.rs#L1521) (`lower_stream_filter_register`)
- **Function symbol**: `lower_stream_filter_register()`


### Lowering notes

- Lowers `stream_filter_register(filter_name, class)` into the user-filter registry helper.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_stream_filter_register`

## Signature summary

```php
function stream_filter_register(string $filter_name, string $class): bool
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Cross-references

- [User reference for `stream_filter_register()`](../../../php/builtins/io/stream_filter_register.md)

