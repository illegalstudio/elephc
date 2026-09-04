---
title: "error_reporting() - internals"
description: "Compiler internals for error_reporting(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 324
---

## `error_reporting()` - internals

## Where it lives

- **Signature**: [`src/builtins/system/error_reporting.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/system/error_reporting.rs)
- **Lowering**: [`src/builtins/semantics.rs`:583](https://github.com/illegalstudio/elephc/blob/main/src/builtins/semantics.rs#L583) (`lower_registry_call`)
- **Function symbol**: `lower_registry_call()`


### Lowering notes

- Uses the `eir_primitive` strategy from the single-source builtin descriptor.
- Emits backend-neutral EIR primitives or a small EIR graph through `BuiltinLoweringContext`.

## Semantic descriptor

- **Target strategy**: `eir_primitive`
- **Validation**: `signature`
- **Result type source**: `declared`
- **Result ownership**: `non_heap`
- **Effects**: `static (2 declared effects)`
- **Requirements**: `static (0 requirements)`
- **Callable policy**: `dynamic`
- **Target support**: `macos-aarch64`, `ios-arm64`, `ios-sim-arm64`, `linux-aarch64`, `linux-x86_64`

## EIR and runtime boundary

- **Typed EIR target**: descriptor-emitted EIR primitives or graph; no opaque builtin call remains.

## Signature summary

```php
function error_reporting(mixed $error_level = null): int
```

## What the type checker enforces

- **Arity**: takes 0–1 arguments (1 optional).

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/core/error_reporting.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/core/error_reporting.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `runtime-state-or-resource`.
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `error_reporting()`](../../../php/builtins/misc/error_reporting.md)
