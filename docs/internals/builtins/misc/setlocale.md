---
title: "setlocale() — internals"
description: "Compiler internals for setlocale(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 337
---

## `setlocale()` — internals

## Where it lives

- **Signature**: [`src/builtins/system/setlocale.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/system/setlocale.rs)
- **Lowering**: [`src/builtins/semantics.rs`:576](https://github.com/illegalstudio/elephc/blob/main/src/builtins/semantics.rs#L576) (`lower_registry_call`)
- **Function symbol**: `lower_registry_call()`


### Lowering notes

- Uses the `runtime_call` strategy from the single-source builtin descriptor.
- Emits the typed EIR target `runtime.setlocale` through `BuiltinLoweringContext`.
- The backend resolves that typed target through `src/codegen/lower_inst/runtime_calls.rs`; PHP builtin names do not participate in dispatch.

## Semantic descriptor

- **Target strategy**: `runtime_call`
- **Validation**: `checker_hook`
- **Result type source**: `checked`
- **Result ownership**: `fresh`
- **Effects**: `static (3 declared effects)`
- **Requirements**: `static (0 requirements)`
- **Callable policy**: `static_only`
- **Target support**: `macos-aarch64`, `ios-arm64`, `ios-sim-arm64`, `linux-aarch64`, `linux-x86_64`

## EIR and runtime boundary

- **Typed EIR target**: `runtime.setlocale`
- **Backend boundary**: `src/codegen/lower_inst/runtime_calls.rs` resolves the typed target without PHP-name dispatch.

## Signature summary

```php
function setlocale(int $category, mixed $locales, ...$rest): mixed
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.
- **Variadic**: collects excess arguments into `$rest`.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/network_env/setlocale.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/network_env/setlocale.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `runtime-state-or-resource`.
- **Dispatch hooks**: `direct`, `values`
- **Variadic**: collects excess arguments into `$rest`.

## Cross-references

- [User reference for `setlocale()`](../../../php/builtins/misc/setlocale.md)
