---
title: "stream_context_get_options() — internals"
description: "Compiler internals for stream_context_get_options(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 210
---

## `stream_context_get_options()` — internals

## Where it lives

- **Signature**: [`src/builtins/io/stream_context_get_options.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/io/stream_context_get_options.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/io.rs`:1251](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/io.rs#L1251) (`lower_stream_context_get_options`)
- **Function symbol**: `lower_stream_context_get_options()`


### Lowering notes

- Lowers `stream_context_get_options(context)`.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_hash_new`
- `__rt_incref`

## Signature summary

```php
function stream_context_get_options(resource $context): array
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/filesystem/stream_context_get_options.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/stream_context_get_options.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `stream_context_get_options()`](../../../php/builtins/io/stream_context_get_options.md)
