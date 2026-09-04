---
title: "user_error() - internals"
description: "Compiler internals for user_error(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 358
---

## `user_error()` - internals

## Where it lives

- **Signature**: [`src/builtins/system/user_error.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/system/user_error.rs)
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
- **Effects**: `static (18 declared effects)`
- **Requirements**: `static (0 requirements)`
- **Callable policy**: `dynamic`
- **Target support**: `macos-aarch64`, `ios-arm64`, `ios-sim-arm64`, `linux-aarch64`, `linux-x86_64`

## EIR and runtime boundary

- **Typed EIR target**: descriptor-emitted EIR primitives or graph; no opaque builtin call remains.

## Signature summary

```php
function user_error(string $message, int $error_level = 1024): bool
```

## What the type checker enforces

- **Arity**: takes 1–2 arguments (1 optional).

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/core/user_error.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/core/user_error.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `runtime-state-or-resource`.
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `user_error()`](../../../php/builtins/misc/user_error.md)
