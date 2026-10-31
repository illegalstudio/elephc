---
title: "intdiv() - internals"
description: "Compiler internals for intdiv(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 642
---

## `intdiv()` - internals

## Where it lives

- **Signature**: [`src/builtins/math/intdiv.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/math/intdiv.rs)
- **Lowering**: [`src/builtins/semantics.rs`:729](https://github.com/illegalstudio/elephc/blob/main/src/builtins/semantics.rs#L729) (`lower_registry_call`)
- **Function symbol**: `lower_registry_call()`


### Lowering notes

- Uses the `eir_graph` strategy from the single-source builtin descriptor.
- Emits backend-neutral EIR primitives or a small EIR graph through `BuiltinLoweringContext`.

## Semantic descriptor

- **Target strategy**: `eir_graph`
- **Validation**: `signature`
- **Result type source**: `declared`
- **Result ownership**: `may_alias_arguments`
- **Effects**: `static (16 declared effects)`
- **Requirements**: `static (0 requirements)`
- **Callable policy**: `static_only`
- **Target support**: `macos-aarch64`, `ios-arm64`, `ios-sim-arm64`, `linux-aarch64`, `linux-x86_64`

## EIR and runtime boundary

- **Typed EIR target**: descriptor-emitted EIR primitives or graph; no opaque builtin call remains.

## Signature summary

```php
function intdiv(int $num1, int $num2): int
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/math/intdiv.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/math/intdiv.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `interpreter-specific-value-semantics`.
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `intdiv()`](../../../php/builtins/math/intdiv.md)
