---
title: "get_required_files() - internals"
description: "Compiler internals for get_required_files(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 342
---

## `get_required_files()` - internals

## Where it lives

- **Signature**: [`src/builtins/system/get_required_files.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/system/get_required_files.rs)
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
function get_required_files(): array
```

## What the type checker enforces

- **Arity**: takes no arguments.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/core/get_required_files.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/core/get_required_files.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `callable-or-reflection`.
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `get_required_files()`](../../../php/builtins/misc/get_required_files.md)
