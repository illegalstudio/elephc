---
title: "array_merge_recursive() — internals"
description: "Compiler internals for array_merge_recursive(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 24
---

## `array_merge_recursive()` — internals

## Where it lives

- **Signature**: [`src/builtins/array/array_merge_recursive.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/array/array_merge_recursive.rs)
- **Lowering**: [`src/builtins/semantics.rs`:425](https://github.com/illegalstudio/elephc/blob/main/src/builtins/semantics.rs#L425) (`lower_registry_call`)
- **Function symbol**: `lower_registry_call()`


### Lowering notes

- Uses the `runtime_call` strategy from the single-source builtin descriptor.
- Emits the typed EIR target `runtime.array_merge_recursive` through `BuiltinLoweringContext`.
- The backend resolves that typed target through `src/codegen/lower_inst/runtime_calls.rs`; PHP builtin names do not participate in dispatch.

## Semantic descriptor

- **Target strategy**: `runtime_call`
- **Validation**: `checker_hook`
- **Result type source**: `checked`
- **Result ownership**: `may_alias_arguments`
- **Effects**: `static (0 declared effects)`
- **Requirements**: `static (0 requirements)`
- **Callable policy**: `static_only`
- **Target support**: `macos-aarch64`, `linux-aarch64`, `linux-x86_64`

## EIR and runtime boundary

- **Typed EIR target**: `runtime.array_merge_recursive`
- **Backend boundary**: `src/codegen/lower_inst/runtime_calls.rs` resolves the typed target without PHP-name dispatch.

## Signature summary

```php
function array_merge_recursive(...$arrays): array
```

## What the type checker enforces

- **Arity**: takes no arguments.
- **Variadic**: collects excess arguments into `$arrays`.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `array_merge_recursive()`](../../../php/builtins/array/array_merge_recursive.md)
