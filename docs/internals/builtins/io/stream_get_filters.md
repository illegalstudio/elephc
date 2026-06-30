---
title: "stream_get_filters() — internals"
description: "Compiler internals for stream_get_filters(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 200
---

## `stream_get_filters()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/io.rs`:1493](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/io.rs#L1493) (`lower_stream_get_filters`)
- **Function symbol**: `lower_stream_get_filters()`


### Lowering notes

- Lowers `stream_get_filters()` to the static built-in filter list.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function stream_get_filters(): array
```

## What the type checker enforces

- **Arity**: takes no arguments.

## Cross-references

- [User reference for `stream_get_filters()`](../../../php/builtins/io/stream_get_filters.md)

