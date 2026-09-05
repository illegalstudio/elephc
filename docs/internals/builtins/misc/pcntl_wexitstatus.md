---
title: "pcntl_wexitstatus() — internals"
description: "Compiler internals for pcntl_wexitstatus(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 353
---

## `pcntl_wexitstatus()` — internals

## Where it lives

- **Signature**: [`src/builtins/system/pcntl_wexitstatus.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/system/pcntl_wexitstatus.rs)
- **Lowering**: [`src/builtins/semantics.rs`:610](https://github.com/illegalstudio/elephc/blob/main/src/builtins/semantics.rs#L610) (`lower_registry_call`)
- **Function symbol**: `lower_registry_call()`


### Lowering notes

- Uses the `runtime_call` strategy from the single-source builtin descriptor.
- Emits the typed EIR target `runtime.pcntl.wexitstatus` through `BuiltinLoweringContext`.
- The backend resolves that typed target through `src/codegen/lower_inst/runtime_calls.rs`; PHP builtin names do not participate in dispatch.

## Semantic descriptor

- **Target strategy**: `runtime_call`
- **Validation**: `shared`
- **Result type source**: `declared`
- **Result ownership**: `fresh`
- **Effects**: `static (0 declared effects)`
- **Requirements**: `static (1 requirements)`
- **Callable policy**: `static_only`
- **Target support**: `macos-aarch64`, `linux-aarch64`, `linux-x86_64`

## EIR and runtime boundary

- **Typed EIR target**: `runtime.pcntl.wexitstatus`
- **Backend boundary**: `src/codegen/lower_inst/runtime_calls.rs` resolves the typed target without PHP-name dispatch.

## Signature summary

```php
function pcntl_wexitstatus(int $status): mixed
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/pcntl/pcntl_wexitstatus.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/pcntl/pcntl_wexitstatus.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `capability-dependent`.
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `pcntl_wexitstatus()`](../../../php/builtins/misc/pcntl_wexitstatus.md)
