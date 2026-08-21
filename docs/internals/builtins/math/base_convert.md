---
title: "base_convert() — internals"
description: "Compiler internals for base_convert(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 278
---

## `base_convert()` — internals

## Where it lives

- **Signature**: [`src/builtins/math/base_convert.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/math/base_convert.rs)
- **Lowering**: [`src/builtins/semantics.rs`:551](https://github.com/illegalstudio/elephc/blob/main/src/builtins/semantics.rs#L551) (`lower_registry_call`)
- **Function symbol**: `lower_registry_call()`


### Lowering notes

- Uses the `runtime_call` strategy from the single-source builtin descriptor.
- Emits the typed EIR target `runtime.base_convert` through `BuiltinLoweringContext`.
- The backend resolves that typed target through `src/codegen/lower_inst/runtime_calls.rs`; PHP builtin names do not participate in dispatch.

## Semantic descriptor

- **Target strategy**: `runtime_call`
- **Validation**: `signature`
- **Result type source**: `declared`
- **Result ownership**: `independent`
- **Effects**: `static (1 declared effects)`
- **Requirements**: `static (0 requirements)`
- **Callable policy**: `static_only`
- **Target support**: `macos-aarch64`, `linux-aarch64`, `linux-x86_64`

## EIR and runtime boundary

- **Typed EIR target**: `runtime.base_convert`
- **Backend boundary**: `src/codegen/lower_inst/runtime_calls.rs` resolves the typed target without PHP-name dispatch.

## Signature summary

```php
function base_convert(string $num, int $from_base, int $to_base): string
```

## What the type checker enforces

- **Arity**: takes exactly 3 arguments.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/math/base_convert.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/math/base_convert.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `interpreter-specific-value-semantics`.
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `base_convert()`](../../../php/builtins/math/base_convert.md)
