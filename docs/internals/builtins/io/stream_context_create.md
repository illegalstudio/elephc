---
title: "stream_context_create() — internals"
description: "Compiler internals for stream_context_create(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 189
---

## `stream_context_create()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/io.rs`:1064](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/io.rs#L1064) (`lower_stream_context_create`)
- **Function symbol**: `lower_stream_context_create()`


### Lowering notes

- Lowers `stream_context_create(options?, params?)`.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function stream_context_create(array $options, array $params): mixed
```

## What the type checker enforces

- **Arity**: takes 0–2 arguments (2 optional).

## Cross-references

- [User reference for `stream_context_create()`](../../../php/builtins/io/stream_context_create.md)

