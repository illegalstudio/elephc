---
title: "buffer_ptr() - internals"
description: "Compiler internals for buffer_ptr(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 76
---

## `buffer_ptr()` - internals

## Where it lives

- **Signature**: [`src/builtins/pointers/buffer_ptr.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/pointers/buffer_ptr.rs)
- **Lowering**: [`src/builtins/semantics.rs`:714](https://github.com/illegalstudio/elephc/blob/main/src/builtins/semantics.rs#L714) (`lower_registry_call`)
- **Function symbol**: `lower_registry_call()`


### Lowering notes

- Uses the `runtime_call` strategy from the single-source builtin descriptor.
- Emits the typed EIR target `runtime.buffer_ptr` through `BuiltinLoweringContext`.
- The backend resolves that typed target through `src/codegen/lower_inst/runtime_calls.rs`; PHP builtin names do not participate in dispatch.

## Semantic descriptor

- **Target strategy**: `runtime_call`
- **Validation**: `checker_hook`
- **Result type source**: `checked`
- **Result ownership**: `non_heap`
- **Effects**: `static (2 declared effects)`
- **Requirements**: `static (0 requirements)`
- **Callable policy**: `static_only`
- **Target support**: `macos-aarch64`, `ios-arm64`, `ios-sim-arm64`, `linux-aarch64`, `linux-x86_64`

## EIR and runtime boundary

- **Typed EIR target**: `runtime.buffer_ptr`
- **Backend boundary**: `src/codegen/lower_inst/runtime_calls.rs` resolves the typed target without PHP-name dispatch.

## Signature summary

```php
function buffer_ptr(buffer $buffer): pointer
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/raw_memory/buffer_ptr.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/raw_memory/buffer_ptr.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `runtime-state-or-resource`.
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `buffer_ptr()`](../../../php/builtins/buffer/buffer_ptr.md)
