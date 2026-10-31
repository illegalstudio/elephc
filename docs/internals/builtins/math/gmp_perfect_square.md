---
title: "gmp_perfect_square() - internals"
description: "Compiler internals for gmp_perfect_square(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 620
---

## `gmp_perfect_square()` - internals

## Where it lives

- **Signature**: [`src/builtins/gmp/gmp_perfect_square.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/gmp/gmp_perfect_square.rs)
- **Lowering**: [`src/builtins/semantics.rs`:719](https://github.com/illegalstudio/elephc/blob/main/src/builtins/semantics.rs#L719) (`lower_registry_call`)
- **Function symbol**: `lower_registry_call()`


### Lowering notes

- Uses the `eir_graph` strategy from the single-source builtin descriptor.
- Emits backend-neutral EIR primitives or a small EIR graph through `BuiltinLoweringContext`.

## Semantic descriptor

- **Target strategy**: `eir_graph`
- **Validation**: `shared`
- **Result type source**: `shared`
- **Result ownership**: `non_heap`
- **Effects**: `static (20 declared effects)`
- **Requirements**: `static (1 requirements)`
- **Callable policy**: `dynamic`
- **Target support**: `macos-aarch64`, `ios-arm64`, `ios-sim-arm64`, `linux-aarch64`, `linux-x86_64`

## EIR and runtime boundary

- **Typed EIR target**: descriptor-emitted EIR primitives or graph; no opaque builtin call remains.

## Signature summary

```php
function gmp_perfect_square(object|string|int $num): bool
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/gmp/gmp_perfect_square.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/gmp/gmp_perfect_square.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `capability-dependent`.
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `gmp_perfect_square()`](../../../php/builtins/math/gmp_perfect_square.md)
