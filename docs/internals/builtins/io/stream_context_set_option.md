---
title: "stream_context_set_option() — internals"
description: "Compiler internals for stream_context_set_option(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 213
---

## `stream_context_set_option()` — internals

## Where it lives

- **Signature**: [`src/builtins/io/stream_context_set_option.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/io/stream_context_set_option.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/io.rs`:1097](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/io.rs#L1097) (`lower_stream_context_set_option`)
- **Function symbol**: `lower_stream_context_set_option()`


### Lowering notes

- Lowers `stream_context_set_option(context, options)` and the four-argument form.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function stream_context_set_option(resource $context, string $wrapper_or_options, string $option_name = null, mixed $value = null): bool
```

## What the type checker enforces

- **Arity**: takes 2–4 arguments (2 optional).

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/filesystem/stream_context_set_option.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/stream_context_set_option.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `stream_context_set_option()`](../../../php/builtins/io/stream_context_set_option.md)
