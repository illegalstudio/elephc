---
title: "stream_wrapper_restore() — internals"
description: "Compiler internals for stream_wrapper_restore(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 210
---

## `stream_wrapper_restore()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/io.rs`:939](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/io.rs#L939) (`lower_stream_wrapper_restore`)
- **Function symbol**: `lower_stream_wrapper_restore()`


### Lowering notes

- Lowers `stream_wrapper_restore(protocol)` as a successful no-op.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function stream_wrapper_restore(string $protocol): bool
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Cross-references

- [User reference for `stream_wrapper_restore()`](../../../php/builtins/io/stream_wrapper_restore.md)

