---
title: "pow() - internals"
description: "Compiler internals for pow(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 657
---

## `pow()` - internals

## Where it lives

- **Signature**: [`src/builtins/math/pow.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/math/pow.rs)
- **Lowering**: [`src/builtins/semantics.rs`:714](https://github.com/illegalstudio/elephc/blob/main/src/builtins/semantics.rs#L714) (`lower_registry_call`)
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
- **Requirements**: `static (0 requirements)`
- **Callable policy**: `dynamic`
- **Target support**: `macos-aarch64`, `ios-arm64`, `ios-sim-arm64`, `linux-aarch64`, `linux-x86_64`

## EIR and runtime boundary

- **Typed EIR target**: descriptor-emitted EIR primitives or graph; no opaque builtin call remains.

## Signature summary

```php
function pow(float $num, float $exponent): float
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/math/pow.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/math/pow.rs) (`eval_builtin!`)
- **Execution**: shared generated-runtime ABI (`RuntimeBuiltinId(12)`).
- **Dispatch hooks**: _none_ (shared runtime dispatch)

## Cross-references

- [User reference for `pow()`](../../../php/builtins/math/pow.md)
