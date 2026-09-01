---
title: "prev() — internals"
description: "Compiler internals for prev(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 62
---

## `prev()` — internals

## Where it lives

- **Signature**: [`src/builtins/array/prev.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/array/prev.rs)
- **Lowering**: [`src/builtins/semantics.rs`:576](https://github.com/illegalstudio/elephc/blob/main/src/builtins/semantics.rs#L576) (`lower_registry_call`)
- **Function symbol**: `lower_registry_call()`


### Lowering notes

- Uses the `runtime_call` strategy from the single-source builtin descriptor.
- Emits the typed EIR target `runtime.array_ptr_seek` through `BuiltinLoweringContext`.
- The backend resolves that typed target through `src/codegen/lower_inst/runtime_calls.rs`; PHP builtin names do not participate in dispatch.

## Semantic descriptor

- **Target strategy**: `runtime_call`
- **Validation**: `checker_hook`
- **Result type source**: `checked`
- **Result ownership**: `fresh`
- **Effects**: `static (16 declared effects)`
- **Requirements**: `static (0 requirements)`
- **Callable policy**: `static_only`
- **Target support**: `macos-aarch64`, `ios-arm64`, `ios-sim-arm64`, `linux-aarch64`, `linux-x86_64`

## EIR and runtime boundary

- **Typed EIR target**: `runtime.array_ptr_seek`
- **Backend boundary**: `src/codegen/lower_inst/runtime_calls.rs` resolves the typed target without PHP-name dispatch.

## Signature summary

```php
function prev(array $array): mixed
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.
- **By-reference parameters**: `$array`.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/array/prev.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/array/prev.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `by-reference-or-lvalue`.
- **Dispatch hooks**: `values`
- **By-reference parameters**: `$array`.

## Cross-references

- [User reference for `prev()`](../../../php/builtins/array/prev.md)
