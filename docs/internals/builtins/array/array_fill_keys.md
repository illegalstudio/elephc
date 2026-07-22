---
title: "array_fill_keys() — internals"
description: "Compiler internals for array_fill_keys(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 10
---

## `array_fill_keys()` — internals

## Where it lives

- **Signature**: [`src/builtins/array/array_fill_keys.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/array/array_fill_keys.rs)
- **Lowering**: [`src/builtins/semantics.rs`:423](https://github.com/illegalstudio/elephc/blob/main/src/builtins/semantics.rs#L423) (`lower_registry_call`)
- **Function symbol**: `lower_registry_call()`


### Lowering notes

- Uses the `runtime_call` strategy from the single-source builtin descriptor.
- Emits the typed EIR target `runtime.array_fill_keys` through `BuiltinLoweringContext`.
- The backend resolves that typed target through `src/codegen/lower_inst/runtime_calls.rs`; PHP builtin names do not participate in dispatch.

## Semantic descriptor

- **Target strategy**: `runtime_call`
- **Validation**: `checker_hook`
- **Result type source**: `checked`
- **Result ownership**: `fresh`
- **Effects**: `static (0 declared effects)`
- **Requirements**: `static (0 requirements)`
- **Callable policy**: `static_only`
- **Target support**: `macos-aarch64`, `linux-aarch64`, `linux-x86_64`

## EIR and runtime boundary

- **Typed EIR target**: `runtime.array_fill_keys`
- **Backend boundary**: `src/codegen/lower_inst/runtime_calls.rs` resolves the typed target without PHP-name dispatch.

## Signature summary

```php
function array_fill_keys(array $keys, mixed $value): array
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/array/array_fill_keys.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/array/array_fill_keys.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `array_fill_keys()`](../../../php/builtins/array/array_fill_keys.md)
