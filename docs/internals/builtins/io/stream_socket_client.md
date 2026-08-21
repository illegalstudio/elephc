---
title: "stream_socket_client() — internals"
description: "Compiler internals for stream_socket_client(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 255
---

## `stream_socket_client()` — internals

## Where it lives

- **Signature**: [`src/builtins/io/stream_socket_client.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/io/stream_socket_client.rs)
- **Lowering**: [`src/builtins/semantics.rs`:551](https://github.com/illegalstudio/elephc/blob/main/src/builtins/semantics.rs#L551) (`lower_registry_call`)
- **Function symbol**: `lower_registry_call()`


### Lowering notes

- Uses the `runtime_call` strategy from the single-source builtin descriptor.
- Emits the typed EIR target `runtime.stream_socket_client` through `BuiltinLoweringContext`.
- The backend resolves that typed target through `src/codegen/lower_inst/runtime_calls.rs`; PHP builtin names do not participate in dispatch.

## Semantic descriptor

- **Target strategy**: `runtime_call`
- **Validation**: `checker_hook`
- **Result type source**: `checked`
- **Result ownership**: `may_alias_arguments`
- **Effects**: `static (16 declared effects)`
- **Requirements**: `static (0 requirements)`
- **Callable policy**: `static_only`
- **Target support**: `macos-aarch64`, `linux-aarch64`, `linux-x86_64`

## EIR and runtime boundary

- **Typed EIR target**: `runtime.stream_socket_client`
- **Backend boundary**: `src/codegen/lower_inst/runtime_calls.rs` resolves the typed target without PHP-name dispatch.

## Signature summary

```php
function stream_socket_client(string $address, mixed $error_code = null, mixed $error_message = null, mixed $timeout = null, int $flags = 4, mixed $context = null): mixed
```

## What the type checker enforces

- **Arity**: takes 1–6 arguments (5 optional).
- **By-reference parameters**: `$error_code`, `$error_message`.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/filesystem/stream_socket_client.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/stream_socket_client.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `by-reference-or-lvalue`.
- **Dispatch hooks**: `direct`, `values`
- **By-reference parameters**: `$error_code`, `$error_message`.

## Cross-references

- [User reference for `stream_socket_client()`](../../../php/builtins/io/stream_socket_client.md)
