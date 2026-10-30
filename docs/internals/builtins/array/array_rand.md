---
title: "array_rand() - internals"
description: "Compiler internals for array_rand(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 33
---

## `array_rand()` - internals

## Where it lives

- **Signature**: [`src/builtins/array/array_rand.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/array/array_rand.rs)
- **Lowering**: [`src/builtins/semantics.rs`:706](https://github.com/illegalstudio/elephc/blob/main/src/builtins/semantics.rs#L706) (`lower_registry_call`)
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
- **Requirements**: `static (1 requirements)`
- **Callable policy**: `dynamic`
- **Target support**: `macos-aarch64`, `ios-arm64`, `ios-sim-arm64`, `linux-aarch64`, `linux-x86_64`

## EIR and runtime boundary

- **Typed EIR target**: descriptor-emitted EIR primitives or graph; no opaque builtin call remains.

## Signature summary

```php
function array_rand(array $array, int $num = 1): int|string|array
```

## What the type checker enforces

- **Arity**: takes 1–2 arguments (1 optional).

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/array/array_rand.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/array/array_rand.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `interpreter-specific-value-semantics`.
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `array_rand()`](../../../php/builtins/array/array_rand.md)
