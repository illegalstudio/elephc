---
title: "memory_get_usage() - internals"
description: "Compiler internals for memory_get_usage(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 706
---

## `memory_get_usage()` - internals

## Where it lives

- **Signature**: [`src/builtins/system/memory_get_usage.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/system/memory_get_usage.rs)
- **Lowering**: [`src/builtins/semantics.rs`:714](https://github.com/illegalstudio/elephc/blob/main/src/builtins/semantics.rs#L714) (`lower_registry_call`)
- **Function symbol**: `lower_registry_call()`


### Lowering notes

- Uses the `runtime_call` strategy from the single-source builtin descriptor.
- Emits the typed EIR target `runtime.memory_get_usage` through `BuiltinLoweringContext`.
- The backend resolves that typed target through `src/codegen/lower_inst/runtime_calls.rs`; PHP builtin names do not participate in dispatch.

## Semantic descriptor

- **Target strategy**: `runtime_call`
- **Validation**: `shared`
- **Result type source**: `declared`
- **Result ownership**: `non_heap`
- **Effects**: `shared`
- **Requirements**: `static (0 requirements)`
- **Callable policy**: `dynamic_target`
- **Target support**: `macos-aarch64`, `ios-arm64`, `ios-sim-arm64`, `linux-aarch64`, `linux-x86_64`

## EIR and runtime boundary

- **Typed EIR target**: `runtime.memory_get_usage`
- **Backend boundary**: `src/codegen/lower_inst/runtime_calls.rs` resolves the typed target without PHP-name dispatch.

## Signature summary

```php
function memory_get_usage(bool $real_usage = false): int
```

## What the type checker enforces

- **Arity**: takes 0–1 arguments (1 optional).

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/core/memory_get_usage.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/core/memory_get_usage.rs) (`eval_builtin!`)
- **Execution**: generated-runtime ABI (`RuntimeBuiltinId(92)`) with a Magician fallback adapter.
- **Adapter reason**: `additional-signature-semantics`.
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `memory_get_usage()`](../../../php/builtins/misc/memory_get_usage.md)
