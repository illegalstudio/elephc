---
title: "memory_get_frees() - internals"
description: "Compiler internals for memory_get_frees(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 704
---

## `memory_get_frees()` - internals

## Where it lives

- **Signature**: [`src/builtins/system/memory_get_frees.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/system/memory_get_frees.rs)
- **Lowering**: [`src/builtins/semantics.rs`:719](https://github.com/illegalstudio/elephc/blob/main/src/builtins/semantics.rs#L719) (`lower_registry_call`)
- **Function symbol**: `lower_registry_call()`


### Lowering notes

- Uses the `runtime_call` strategy from the single-source builtin descriptor.
- Emits the typed EIR target `runtime.memory_get_frees` through `BuiltinLoweringContext`.
- The backend resolves that typed target through `src/codegen/lower_inst/runtime_calls.rs`; PHP builtin names do not participate in dispatch.

## Semantic descriptor

- **Target strategy**: `runtime_call`
- **Validation**: `signature`
- **Result type source**: `declared`
- **Result ownership**: `non_heap`
- **Effects**: `static (2 declared effects)`
- **Requirements**: `static (0 requirements)`
- **Callable policy**: `dynamic_target`
- **Target support**: `macos-aarch64`, `ios-arm64`, `ios-sim-arm64`, `linux-aarch64`, `linux-x86_64`

## EIR and runtime boundary

- **Typed EIR target**: `runtime.memory_get_frees`
- **Backend boundary**: `src/codegen/lower_inst/runtime_calls.rs` resolves the typed target without PHP-name dispatch.

## Signature summary

```php
function memory_get_frees(): int
```

## What the type checker enforces

- **Arity**: takes no arguments.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/core/memory_get_frees.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/core/memory_get_frees.rs) (`eval_builtin!`)
- **Execution**: shared generated-runtime ABI (`RuntimeBuiltinId(90)`).
- **Dispatch hooks**: _none_ (shared runtime dispatch)

## Cross-references

- [User reference for `memory_get_frees()`](../../../php/builtins/misc/memory_get_frees.md)
