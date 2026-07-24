---
title: "stream_select() — internals"
description: "Compiler internals for stream_select(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 229
---

## `stream_select()` — internals

## Where it lives

- **Signature**: [`src/builtins/io/stream_select.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/io/stream_select.rs)
- **Lowering**: [`src/builtins/semantics.rs`:425](https://github.com/illegalstudio/elephc/blob/main/src/builtins/semantics.rs#L425) (`lower_registry_call`)
- **Function symbol**: `lower_registry_call()`


### Lowering notes

- Uses the `runtime_call` strategy from the single-source builtin descriptor.
- Emits the typed EIR target `runtime.stream_select` through `BuiltinLoweringContext`.
- The backend resolves that typed target through `src/codegen/lower_inst/runtime_calls.rs`; PHP builtin names do not participate in dispatch.

## Semantic descriptor

- **Target strategy**: `runtime_call`
- **Validation**: `signature`
- **Result type source**: `declared`
- **Result ownership**: `may_alias_arguments`
- **Effects**: `static (16 declared effects)`
- **Requirements**: `static (0 requirements)`
- **Callable policy**: `static_only`
- **Target support**: `macos-aarch64`, `linux-aarch64`, `linux-x86_64`

## EIR and runtime boundary

- **Typed EIR target**: `runtime.stream_select`
- **Backend boundary**: `src/codegen/lower_inst/runtime_calls.rs` resolves the typed target without PHP-name dispatch.

## Signature summary

```php
function stream_select(array &$read, array &$write, array &$except, int $seconds, int $microseconds = 0): int
```

## What the type checker enforces

- **Arity**: takes 4–5 arguments (1 optional).
- **By-reference parameters**: `$read`, `$write`, `$except`.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/filesystem/stream_select.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/stream_select.rs) (`eval_builtin!`)
- **Dispatch hooks**: `values`
- **By-reference parameters**: `$read`, `$write`, `$except`.

## Cross-references

- [User reference for `stream_select()`](../../../php/builtins/io/stream_select.md)
