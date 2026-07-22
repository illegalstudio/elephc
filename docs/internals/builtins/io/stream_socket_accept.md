---
title: "stream_socket_accept() — internals"
description: "Compiler internals for stream_socket_accept(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 235
---

## `stream_socket_accept()` — internals

## Where it lives

- **Signature**: [`src/builtins/io/stream_socket_accept.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/io/stream_socket_accept.rs)
- **Lowering**: [`src/builtins/semantics.rs`:423](https://github.com/illegalstudio/elephc/blob/main/src/builtins/semantics.rs#L423) (`lower_registry_call`)
- **Function symbol**: `lower_registry_call()`


### Lowering notes

- Uses the `runtime_call` strategy from the single-source builtin descriptor.
- Emits the typed EIR target `runtime.stream_socket_accept` through `BuiltinLoweringContext`.
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

- **Typed EIR target**: `runtime.stream_socket_accept`
- **Backend boundary**: `src/codegen/lower_inst/runtime_calls.rs` resolves the typed target without PHP-name dispatch.

## Signature summary

```php
function stream_socket_accept(resource $socket, float $timeout = null, string $peer_name = null): mixed
```

## What the type checker enforces

- **Arity**: takes 1–3 arguments (2 optional).
- **By-reference parameters**: `$peer_name`.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/filesystem/stream_socket_accept.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/stream_socket_accept.rs) (`eval_builtin!`)
- **Dispatch hooks**: `values`
- **By-reference parameters**: `$peer_name`.

## Cross-references

- [User reference for `stream_socket_accept()`](../../../php/builtins/io/stream_socket_accept.md)
