---
title: "stream_context_get_params() — internals"
description: "Compiler internals for stream_context_get_params(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 211
---

## `stream_context_get_params()` — internals

## Where it lives

- **Signature**: [`src/builtins/io/stream_context_get_params.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/io/stream_context_get_params.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/io.rs`:1290](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/io.rs#L1290) (`lower_stream_context_get_params`)
- **Function symbol**: `lower_stream_context_get_params()`


### Lowering notes

- Lowers `stream_context_get_params(context)` to an empty associative hash.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function stream_context_get_params(resource $context): array
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/filesystem/stream_context_get_params.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/stream_context_get_params.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `stream_context_get_params()`](../../../php/builtins/io/stream_context_get_params.md)
