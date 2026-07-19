---
title: "stream_context_create() — internals"
description: "Compiler internals for stream_context_create(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 208
---

## `stream_context_create()` — internals

## Where it lives

- **Signature**: [`src/builtins/io/stream_context_create.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/io/stream_context_create.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/io.rs`:1063](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/io.rs#L1063) (`lower_stream_context_create`)
- **Function symbol**: `lower_stream_context_create()`


### Lowering notes

- Lowers `stream_context_create(options?, params?)`.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function stream_context_create(array $options = null, array $params = null): mixed
```

## What the type checker enforces

- **Arity**: takes 0–2 arguments (2 optional).

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/filesystem/stream_context_create.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/stream_context_create.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `stream_context_create()`](../../../php/builtins/io/stream_context_create.md)
