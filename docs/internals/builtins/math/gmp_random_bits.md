---
title: "gmp_random_bits() - internals"
description: "Compiler internals for gmp_random_bits(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 625
---

## `gmp_random_bits()` - internals

## Where it lives

- **Signature**: [`src/builtins/gmp/gmp_random_bits.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/gmp/gmp_random_bits.rs)
- **Lowering**: [`src/builtins/semantics.rs`:719](https://github.com/illegalstudio/elephc/blob/main/src/builtins/semantics.rs#L719) (`lower_registry_call`)
- **Function symbol**: `lower_registry_call()`


### Lowering notes

- Uses the `eir_graph` strategy from the single-source builtin descriptor.
- Emits backend-neutral EIR primitives or a small EIR graph through `BuiltinLoweringContext`.

## Semantic descriptor

- **Target strategy**: `eir_graph`
- **Validation**: `shared`
- **Result type source**: `shared`
- **Result ownership**: `fresh`
- **Effects**: `static (20 declared effects)`
- **Requirements**: `static (1 requirements)`
- **Callable policy**: `dynamic`
- **Target support**: `macos-aarch64`, `ios-arm64`, `ios-sim-arm64`, `linux-aarch64`, `linux-x86_64`

## EIR and runtime boundary

- **Typed EIR target**: descriptor-emitted EIR primitives or graph; no opaque builtin call remains.

## Signature summary

```php
function gmp_random_bits(int $bits): object
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/gmp/gmp_random_bits.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/gmp/gmp_random_bits.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `capability-dependent`.
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `gmp_random_bits()`](../../../php/builtins/math/gmp_random_bits.md)
