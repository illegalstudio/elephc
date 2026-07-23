---
title: "stream_socket_enable_crypto() — internals"
description: "Compiler internals for stream_socket_enable_crypto(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 237
---

## `stream_socket_enable_crypto()` — internals

## Where it lives

- **Signature**: [`src/builtins/io/stream_socket_enable_crypto.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/io/stream_socket_enable_crypto.rs)
- **Lowering**: [`src/builtins/semantics.rs`:425](https://github.com/illegalstudio/elephc/blob/main/src/builtins/semantics.rs#L425) (`lower_registry_call`)
- **Function symbol**: `lower_registry_call()`


### Lowering notes

- Uses the `runtime_call` strategy from the single-source builtin descriptor.
- Emits the typed EIR target `runtime.stream_socket_enable_crypto` through `BuiltinLoweringContext`.
- The backend resolves that typed target through `src/codegen/lower_inst/runtime_calls.rs`; PHP builtin names do not participate in dispatch.

## Semantic descriptor

- **Target strategy**: `runtime_call`
- **Validation**: `checker_hook`
- **Result type source**: `checked`
- **Result ownership**: `may_alias_arguments`
- **Effects**: `static (16 declared effects)`
- **Requirements**: `static (1 requirements)`
- **Callable policy**: `static_only`
- **Target support**: `macos-aarch64`, `linux-aarch64`, `linux-x86_64`

## EIR and runtime boundary

- **Typed EIR target**: `runtime.stream_socket_enable_crypto`
- **Backend boundary**: `src/codegen/lower_inst/runtime_calls.rs` resolves the typed target without PHP-name dispatch.

## Signature summary

```php
function stream_socket_enable_crypto(resource $stream, bool $enable, int $crypto_method = null, resource $session_stream = null): mixed
```

## What the type checker enforces

- **Arity**: takes 2–4 arguments (2 optional).

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/filesystem/stream_socket_enable_crypto.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/stream_socket_enable_crypto.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `stream_socket_enable_crypto()`](../../../php/builtins/io/stream_socket_enable_crypto.md)
