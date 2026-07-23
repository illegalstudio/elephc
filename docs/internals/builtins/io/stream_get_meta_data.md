---
title: "stream_get_meta_data() — internals"
description: "Compiler internals for stream_get_meta_data(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 223
---

## `stream_get_meta_data()` — internals

## Where it lives

- **Signature**: [`src/builtins/io/stream_get_meta_data.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/io/stream_get_meta_data.rs)
- **Lowering**: [`src/builtins/semantics.rs`:425](https://github.com/illegalstudio/elephc/blob/main/src/builtins/semantics.rs#L425) (`lower_registry_call`)
- **Function symbol**: `lower_registry_call()`


### Lowering notes

- Uses the `runtime_call` strategy from the single-source builtin descriptor.
- Emits the typed EIR target `runtime.stream_get_meta_data` through `BuiltinLoweringContext`.
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

- **Typed EIR target**: `runtime.stream_get_meta_data`
- **Backend boundary**: `src/codegen/lower_inst/runtime_calls.rs` resolves the typed target without PHP-name dispatch.

## Signature summary

```php
function stream_get_meta_data(resource $stream): array
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/filesystem/stream_get_meta_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/stream_get_meta_data.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `stream_get_meta_data()`](../../../php/builtins/io/stream_get_meta_data.md)
