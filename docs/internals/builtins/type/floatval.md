---
title: "floatval() — internals"
description: "Compiler internals for floatval(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 500
---

## `floatval()` — internals

## Where it lives

- **Signature**: [`src/builtins/types/floatval.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/types/floatval.rs)
- **Lowering**: [`src/builtins/semantics.rs`:576](https://github.com/illegalstudio/elephc/blob/main/src/builtins/semantics.rs#L576) (`lower_registry_call`)
- **Function symbol**: `lower_registry_call()`


### Lowering notes

- Uses the `eir_primitive` strategy from the single-source builtin descriptor.
- Emits backend-neutral EIR primitives or a small EIR graph through `BuiltinLoweringContext`.

## Semantic descriptor

- **Target strategy**: `eir_primitive`
- **Validation**: `signature`
- **Result type source**: `declared`
- **Result ownership**: `non_heap`
- **Effects**: `shared`
- **Requirements**: `static (0 requirements)`
- **Callable policy**: `dynamic`
- **Target support**: `macos-aarch64`, `ios-arm64`, `ios-sim-arm64`, `linux-aarch64`, `linux-x86_64`

## EIR and runtime boundary

- **Typed EIR target**: descriptor-emitted EIR primitives or graph; no opaque builtin call remains.

## Signature summary

```php
function floatval(mixed $value): float
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/types/floatval.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/types/floatval.rs) (`eval_builtin!`)
- **Execution**: shared generated-runtime ABI (`RuntimeBuiltinId(2)`).
- **Dispatch hooks**: _none_ (shared runtime dispatch)

## Cross-references

- [User reference for `floatval()`](../../../php/builtins/type/floatval.md)
