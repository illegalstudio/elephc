---
title: "stream_wrapper_register() — internals"
description: "Compiler internals for stream_wrapper_register(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 230
---

## `stream_wrapper_register()` — internals

## Where it lives

- **Signature**: [`src/builtins/io/stream_wrapper_register.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/io/stream_wrapper_register.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/io.rs`:1000](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/io.rs#L1000) (`lower_stream_wrapper_register`)
- **Function symbol**: `lower_stream_wrapper_register()`


### Lowering notes

- Lowers `stream_wrapper_register(protocol, class, flags?)`.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_stream_wrapper_register`

## Signature summary

```php
function stream_wrapper_register(string $protocol, string $class, int $flags = 0): bool
```

## What the type checker enforces

- **Arity**: takes 2–3 arguments (1 optional).

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/filesystem/stream_wrapper_register.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/stream_wrapper_register.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `stream_wrapper_register()`](../../../php/builtins/io/stream_wrapper_register.md)
