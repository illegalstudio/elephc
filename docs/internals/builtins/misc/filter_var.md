---
title: "filter_var() - internals"
description: "Compiler internals for filter_var(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 694
---

## `filter_var()` - internals

## Where it lives

- **Signature**: [`src/builtins/system/filter_var.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/system/filter_var.rs)
- **Lowering**: [`src/builtins/semantics.rs`:752](https://github.com/illegalstudio/elephc/blob/main/src/builtins/semantics.rs#L752) (`lower_registry_call`)
- **Function symbol**: `lower_registry_call()`


### Lowering notes

- Uses the `eir_graph` strategy from the single-source builtin descriptor.
- Emits backend-neutral EIR primitives or a small EIR graph through `BuiltinLoweringContext`.

## Semantic descriptor

- **Target strategy**: `eir_graph`
- **Validation**: `checker_hook`
- **Result type source**: `checked`
- **Result ownership**: `fresh`
- **Effects**: `shared`
- **Requirements**: `static (0 requirements)`
- **Callable policy**: `static_only`
- **Target support**: `macos-aarch64`, `ios-arm64`, `ios-sim-arm64`, `linux-aarch64`, `linux-x86_64`

## EIR and runtime boundary

- **Typed EIR target**: descriptor-emitted EIR primitives or graph; no opaque builtin call remains.

## Signature summary

```php
function filter_var(mixed $value, int $filter = 516, mixed $options = 0): mixed
```

## What the type checker enforces

- **Arity**: takes 1–3 arguments (2 optional).

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/types/filter_var.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/types/filter_var.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `runtime-state-or-resource`.
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `filter_var()`](../../../php/builtins/misc/filter_var.md)
