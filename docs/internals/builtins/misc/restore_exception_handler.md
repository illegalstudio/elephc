---
title: "restore_exception_handler() - internals"
description: "Compiler internals for restore_exception_handler(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 351
---

## `restore_exception_handler()` - internals

## Where it lives

- **Signature**: [`src/builtins/system/restore_exception_handler.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/system/restore_exception_handler.rs)
- **Lowering**: [`src/builtins/semantics.rs`:588](https://github.com/illegalstudio/elephc/blob/main/src/builtins/semantics.rs#L588) (`lower_registry_call`)
- **Function symbol**: `lower_registry_call()`


### Lowering notes

- Uses the `eir_primitive` strategy from the single-source builtin descriptor.
- Emits backend-neutral EIR primitives or a small EIR graph through `BuiltinLoweringContext`.

## Semantic descriptor

- **Target strategy**: `eir_primitive`
- **Validation**: `signature`
- **Result type source**: `declared`
- **Result ownership**: `non_heap`
- **Effects**: `static (8 declared effects)`
- **Requirements**: `static (0 requirements)`
- **Callable policy**: `dynamic`
- **Target support**: `macos-aarch64`, `ios-arm64`, `ios-sim-arm64`, `linux-aarch64`, `linux-x86_64`

## EIR and runtime boundary

- **Typed EIR target**: descriptor-emitted EIR primitives or graph; no opaque builtin call remains.

## Signature summary

```php
function restore_exception_handler(): bool
```

## What the type checker enforces

- **Arity**: takes no arguments.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/core/restore_exception_handler.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/core/restore_exception_handler.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `runtime-state-or-resource`.
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `restore_exception_handler()`](../../../php/builtins/misc/restore_exception_handler.md)
