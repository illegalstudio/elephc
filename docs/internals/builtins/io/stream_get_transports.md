---
title: "stream_get_transports() — internals"
description: "Compiler internals for stream_get_transports(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 203
---

## `stream_get_transports()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/io.rs`:1477](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/io.rs#L1477) (`lower_stream_get_transports`)
- **Function symbol**: `lower_stream_get_transports()`


### Lowering notes

- Lowers `stream_get_transports()` to the static transport list.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function stream_get_transports(): array
```

## What the type checker enforces

- **Arity**: takes no arguments.

## Cross-references

- [User reference for `stream_get_transports()`](../../../php/builtins/io/stream_get_transports.md)

