---
title: "pcntl_getcpuaffinity() — internals"
description: "Compiler internals for pcntl_getcpuaffinity(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 334
---

## `pcntl_getcpuaffinity()` — internals

## Where it lives

- **Signature**: [`src/builtins/system/pcntl_getcpuaffinity.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/system/pcntl_getcpuaffinity.rs)
- **Lowering**: [`src/builtins/semantics.rs`:611](https://github.com/illegalstudio/elephc/blob/main/src/builtins/semantics.rs#L611) (`lower_registry_call`)
- **Function symbol**: `lower_registry_call()`


### Lowering notes

- Uses the `runtime_call` strategy from the single-source builtin descriptor.
- Emits the typed EIR target `runtime.pcntl.getcpuaffinity` through `BuiltinLoweringContext`.
- The backend resolves that typed target through `src/codegen/lower_inst/runtime_calls.rs`; PHP builtin names do not participate in dispatch.

## Semantic descriptor

- **Target strategy**: `runtime_call`
- **Validation**: `checker_hook`
- **Result type source**: `checked`
- **Result ownership**: `fresh`
- **Effects**: `static (3 declared effects)`
- **Requirements**: `static (1 requirements)`
- **Callable policy**: `static_only`
- **Target support**: `linux-aarch64`, `linux-x86_64`

## EIR and runtime boundary

- **Typed EIR target**: `runtime.pcntl.getcpuaffinity`
- **Backend boundary**: `src/codegen/lower_inst/runtime_calls.rs` resolves the typed target without PHP-name dispatch.

## Signature summary

```php
function pcntl_getcpuaffinity(int $process_id = null): mixed
```

## What the type checker enforces

- **Arity**: takes 0–1 arguments (1 optional).

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/pcntl/pcntl_getcpuaffinity.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/pcntl/pcntl_getcpuaffinity.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `capability-dependent`.
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `pcntl_getcpuaffinity()`](../../../php/builtins/misc/pcntl_getcpuaffinity.md)
