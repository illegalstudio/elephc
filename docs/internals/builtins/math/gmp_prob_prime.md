---
title: "gmp_prob_prime() - internals"
description: "Compiler internals for gmp_prob_prime(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 624
---

## `gmp_prob_prime()` - internals

## Where it lives

- **Signature**: [`src/builtins/gmp/gmp_prob_prime.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/gmp/gmp_prob_prime.rs)
- **Lowering**: [`src/builtins/semantics.rs`:729](https://github.com/illegalstudio/elephc/blob/main/src/builtins/semantics.rs#L729) (`lower_registry_call`)
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
function gmp_prob_prime(object|string|int $num, int $repetitions = 10): int
```

## What the type checker enforces

- **Arity**: takes 1–2 arguments (1 optional).

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/gmp/gmp_prob_prime.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/gmp/gmp_prob_prime.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `capability-dependent`.
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `gmp_prob_prime()`](../../../php/builtins/math/gmp_prob_prime.md)
