---
title: "intval() — internals"
description: "Compiler internals for intval(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 564
---

## `intval()` — internals

## Where it lives

- **Signature**: [`src/builtins/types/intval.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/types/intval.rs)
- **Lowering**: [`src/builtins/semantics.rs`:560](https://github.com/illegalstudio/elephc/blob/main/src/builtins/semantics.rs#L560) (`lower_registry_call`)
- **Function symbol**: `lower_registry_call()`


### Lowering notes

- Uses the `eir_graph` strategy from the single-source builtin descriptor.
- Emits backend-neutral EIR primitives or a small EIR graph through `BuiltinLoweringContext`.

## Semantic descriptor

- **Target strategy**: `eir_graph`
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
function intval(mixed $value, int $base = 10): int
```

## What the type checker enforces

- **Arity**: takes 1–2 arguments (1 optional).

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/types/intval.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/types/intval.rs) (`eval_builtin!`)
- **Execution**: generated-runtime ABI (`RuntimeBuiltinId(3)`) with a Magician fallback adapter.
- **Adapter reason**: `additional-signature-semantics`.
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `intval()`](../../../php/builtins/type/intval.md)
