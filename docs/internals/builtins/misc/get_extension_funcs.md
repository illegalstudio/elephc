---
title: "get_extension_funcs() - internals"
description: "Compiler internals for get_extension_funcs(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 338
---

## `get_extension_funcs()` - internals

## Where it lives

- **Signature**: [`src/builtins/system/get_extension_funcs.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/system/get_extension_funcs.rs)
- **Lowering**: [`src/builtins/semantics.rs`:588](https://github.com/illegalstudio/elephc/blob/main/src/builtins/semantics.rs#L588) (`lower_registry_call`)
- **Function symbol**: `lower_registry_call()`


### Lowering notes

- Uses the `eir_primitive` strategy from the single-source builtin descriptor.
- Emits backend-neutral EIR primitives or a small EIR graph through `BuiltinLoweringContext`.

## Semantic descriptor

- **Target strategy**: `eir_primitive`
- **Validation**: `checker_hook`
- **Result type source**: `shared`
- **Result ownership**: `fresh`
- **Effects**: `static (7 declared effects)`
- **Requirements**: `static (0 requirements)`
- **Callable policy**: `dynamic`
- **Target support**: `macos-aarch64`, `ios-arm64`, `ios-sim-arm64`, `linux-aarch64`, `linux-x86_64`

## EIR and runtime boundary

- **Typed EIR target**: descriptor-emitted EIR primitives or graph; no opaque builtin call remains.

## Signature summary

```php
function get_extension_funcs(string $extension): mixed
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/core/get_extension_funcs.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/core/get_extension_funcs.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `callable-or-reflection`.
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `get_extension_funcs()`](../../../php/builtins/misc/get_extension_funcs.md)
