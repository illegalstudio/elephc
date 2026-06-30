---
title: "stream_is_local() — internals"
description: "Compiler internals for stream_is_local(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 205
---

## `stream_is_local()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/io.rs`:2103](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/io.rs#L2103) (`lower_stream_is_local`)
- **Function symbol**: `lower_stream_is_local()`


### Lowering notes

- Lowers `stream_is_local(stream)` as a true predicate after evaluating its argument.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function stream_is_local(resource $stream): bool
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Cross-references

- [User reference for `stream_is_local()`](../../../php/builtins/io/stream_is_local.md)

