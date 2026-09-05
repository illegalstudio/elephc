---
title: "debug_print_backtrace() - internals"
description: "Compiler internals for debug_print_backtrace(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 320
---

## `debug_print_backtrace()` - internals

## Where it lives

- **Signature**: [`src/builtins/system/debug_print_backtrace.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/system/debug_print_backtrace.rs)
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
- **Effects**: `static (20 declared effects)`
- **Requirements**: `static (0 requirements)`
- **Callable policy**: `dynamic`
- **Target support**: `macos-aarch64`, `ios-arm64`, `ios-sim-arm64`, `linux-aarch64`, `linux-x86_64`

## EIR and runtime boundary

- **Typed EIR target**: descriptor-emitted EIR primitives or graph; no opaque builtin call remains.

## Signature summary

```php
function debug_print_backtrace(int $options = 0, int $limit = 0): void
```

## What the type checker enforces

- **Arity**: takes 0–2 arguments (2 optional).

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/core/debug_print_backtrace.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/core/debug_print_backtrace.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `callable-or-reflection`.
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `debug_print_backtrace()`](../../../php/builtins/misc/debug_print_backtrace.md)
