---
title: "get_extension_funcs() — internals"
description: "Compiler internals for get_extension_funcs(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 328
---

## `get_extension_funcs()` — internals

## Where it lives

- **Signature**: [`src/builtins/system/get_extension_funcs.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/system/get_extension_funcs.rs)
- **Lowering**: [`src/builtins/semantics.rs`:576](https://github.com/illegalstudio/elephc/blob/main/src/builtins/semantics.rs#L576) (`lower_registry_call`)
- **Function symbol**: `lower_registry_call()`


### Lowering notes

- Uses the `runtime_call` strategy from the single-source builtin descriptor.
- Emits the typed EIR target `runtime.get_extension_funcs` through `BuiltinLoweringContext`.
- The backend resolves that typed target through `src/codegen/lower_inst/runtime_calls.rs`; PHP builtin names do not participate in dispatch.

## Semantic descriptor

- **Target strategy**: `runtime_call`
- **Validation**: `checker_hook`
- **Result type source**: `checked`
- **Result ownership**: `may_alias_arguments`
- **Effects**: `static (16 declared effects)`
- **Requirements**: `static (0 requirements)`
- **Callable policy**: `static_only`
- **Target support**: `macos-aarch64`, `ios-arm64`, `ios-sim-arm64`, `linux-aarch64`, `linux-x86_64`

## EIR and runtime boundary

- **Typed EIR target**: `runtime.get_extension_funcs`
- **Backend boundary**: `src/codegen/lower_inst/runtime_calls.rs` resolves the typed target without PHP-name dispatch.

## Signature summary

```php
function get_extension_funcs(string $extension): mixed
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/network_env/get_extension_funcs.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/network_env/get_extension_funcs.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `runtime-state-or-resource`.
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `get_extension_funcs()`](../../../php/builtins/misc/get_extension_funcs.md)
