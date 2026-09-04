---
title: "get_mangled_object_vars() - internals"
description: "Compiler internals for get_mangled_object_vars(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 341
---

## `get_mangled_object_vars()` - internals

## Where it lives

- **Signature**: [`src/builtins/system/get_mangled_object_vars.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/system/get_mangled_object_vars.rs)
- **Lowering**: [`src/builtins/semantics.rs`:583](https://github.com/illegalstudio/elephc/blob/main/src/builtins/semantics.rs#L583) (`lower_registry_call`)
- **Function symbol**: `lower_registry_call()`


### Lowering notes

- Uses the `eir_primitive` strategy from the single-source builtin descriptor.
- Emits backend-neutral EIR primitives or a small EIR graph through `BuiltinLoweringContext`.

## Semantic descriptor

- **Target strategy**: `eir_primitive`
- **Validation**: `checker_hook`
- **Result type source**: `checked`
- **Result ownership**: `fresh`
- **Effects**: `static (7 declared effects)`
- **Requirements**: `static (0 requirements)`
- **Callable policy**: `dynamic`
- **Target support**: `macos-aarch64`, `ios-arm64`, `ios-sim-arm64`, `linux-aarch64`, `linux-x86_64`

## EIR and runtime boundary

- **Typed EIR target**: descriptor-emitted EIR primitives or graph; no opaque builtin call remains.

## Signature summary

```php
function get_mangled_object_vars(mixed $object): array
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/core/get_mangled_object_vars.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/core/get_mangled_object_vars.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `callable-or-reflection`.
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `get_mangled_object_vars()`](../../../php/builtins/misc/get_mangled_object_vars.md)
