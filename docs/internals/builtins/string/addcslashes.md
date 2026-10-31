---
title: "addcslashes() - internals"
description: "Compiler internals for addcslashes(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 859
---

## `addcslashes()` - internals

## Where it lives

- **Signature**: [`src/builtins/string/addcslashes.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/string/addcslashes.rs)
- **Lowering**: [`src/builtins/semantics.rs`:714](https://github.com/illegalstudio/elephc/blob/main/src/builtins/semantics.rs#L714) (`lower_registry_call`)
- **Function symbol**: `lower_registry_call()`


### Lowering notes

- Uses the `runtime_call` strategy from the single-source builtin descriptor.
- Emits the typed EIR target `runtime.addcslashes` through `BuiltinLoweringContext`.
- The backend resolves that typed target through `src/codegen/lower_inst/runtime_calls.rs`; PHP builtin names do not participate in dispatch.

## Semantic descriptor

- **Target strategy**: `runtime_call`
- **Validation**: `signature`
- **Result type source**: `declared`
- **Result ownership**: `fresh`
- **Effects**: `static (0 declared effects)`
- **Requirements**: `static (0 requirements)`
- **Callable policy**: `dynamic_target`
- **Target support**: `macos-aarch64`, `ios-arm64`, `ios-sim-arm64`, `linux-aarch64`, `linux-x86_64`

## EIR and runtime boundary

- **Typed EIR target**: `runtime.addcslashes`
- **Backend boundary**: `src/codegen/lower_inst/runtime_calls.rs` resolves the typed target without PHP-name dispatch.

## Signature summary

```php
function addcslashes(string $string, string $characters): string
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/string/addcslashes.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/addcslashes.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `interpreter-specific-value-semantics`.
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `addcslashes()`](../../../php/builtins/string/addcslashes.md)
