---
title: "max() - internals"
description: "Compiler internals for max(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 650
---

## `max()` - internals

## Where it lives

- **Signature**: [`src/builtins/math/max.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/math/max.rs)
- **Lowering**: [`src/builtins/semantics.rs`:729](https://github.com/illegalstudio/elephc/blob/main/src/builtins/semantics.rs#L729) (`lower_registry_call`)
- **Function symbol**: `lower_registry_call()`


### Lowering notes

- Uses the `eir_graph` strategy from the single-source builtin descriptor.
- Emits backend-neutral EIR primitives or a small EIR graph through `BuiltinLoweringContext`.

## Semantic descriptor

- **Target strategy**: `eir_graph`
- **Validation**: `checker_hook`
- **Result type source**: `checked`
- **Result ownership**: `may_alias_arguments`
- **Effects**: `static (20 declared effects)`
- **Requirements**: `static (0 requirements)`
- **Callable policy**: `dynamic_target`
- **Target support**: `macos-aarch64`, `ios-arm64`, `ios-sim-arm64`, `linux-aarch64`, `linux-x86_64`

## EIR and runtime boundary

- **Typed EIR target**: descriptor-emitted EIR primitives or graph; no opaque builtin call remains.

## Signature summary

```php
function max(mixed $value, ...$values): mixed
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.
- **Variadic**: collects excess arguments into `$values`.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/math/max.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/math/max.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `interpreter-specific-value-semantics`.
- **Dispatch hooks**: `direct`, `values`
- **Variadic**: collects excess arguments into `$values`.

## Cross-references

- [User reference for `max()`](../../../php/builtins/math/max.md)
