---
title: "is_file() — internals"
description: "Compiler internals for is_file(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 138
---

## `is_file()` — internals

## Where it lives

- **Signature**: [`src/builtins/io/is_file.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/io/is_file.rs)
- **Lowering**: [`src/builtins/semantics.rs`:576](https://github.com/illegalstudio/elephc/blob/main/src/builtins/semantics.rs#L576) (`lower_registry_call`)
- **Function symbol**: `lower_registry_call()`


### Lowering notes

- Uses the `runtime_call` strategy from the single-source builtin descriptor.
- Emits the typed EIR target `runtime.is_file` through `BuiltinLoweringContext`.
- The backend resolves that typed target through `src/codegen/lower_inst/runtime_calls.rs`; PHP builtin names do not participate in dispatch.

## Semantic descriptor

- **Target strategy**: `runtime_call`
- **Validation**: `signature`
- **Result type source**: `declared`
- **Result ownership**: `may_alias_arguments`
- **Effects**: `static (16 declared effects)`
- **Requirements**: `static (0 requirements)`
- **Callable policy**: `static_only`
- **Target support**: `macos-aarch64`, `ios-arm64`, `ios-sim-arm64`, `linux-aarch64`, `linux-x86_64`

## EIR and runtime boundary

- **Typed EIR target**: `runtime.is_file`
- **Backend boundary**: `src/codegen/lower_inst/runtime_calls.rs` resolves the typed target without PHP-name dispatch.

## Signature summary

```php
function is_file(string $filename): bool
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/filesystem/is_file.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/is_file.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `runtime-state-or-resource`.
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `is_file()`](../../../php/builtins/filesystem/is_file.md)
