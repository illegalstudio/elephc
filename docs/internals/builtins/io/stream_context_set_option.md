---
title: "stream_context_set_option() — internals"
description: "Compiler internals for stream_context_set_option(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 179
---

## `stream_context_set_option()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/io.rs`:1098](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/io.rs#L1098) (`lower_stream_context_set_option`)
- **Function symbol**: `lower_stream_context_set_option()`


### Lowering notes

- Lowers `stream_context_set_option(context, options)` and the four-argument form.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function stream_context_set_option(resource $context, string $wrapper_or_options, string $option_name, mixed $value): bool
```

## What the type checker enforces

- **Arity**: takes 2–4 arguments (2 optional).

## Cross-references

- [User reference for `stream_context_set_option()`](../../../php/builtins/io/stream_context_set_option.md)

