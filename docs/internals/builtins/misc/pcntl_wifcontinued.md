---
title: "pcntl_wifcontinued() — internals"
description: "Compiler internals for pcntl_wifcontinued(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 353
---

## `pcntl_wifcontinued()` — internals

## Where it lives

- **Signature**: [`src/builtins/system/pcntl_wifcontinued.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/system/pcntl_wifcontinued.rs)
- **Lowering**: [`src/builtins/semantics.rs`:611](https://github.com/illegalstudio/elephc/blob/main/src/builtins/semantics.rs#L611) (`lower_registry_call`)
- **Function symbol**: `lower_registry_call()`


### Lowering notes

- Uses the `runtime_call` strategy from the single-source builtin descriptor.
- Emits the typed EIR target `runtime.pcntl.wifcontinued` through `BuiltinLoweringContext`.
- The backend resolves that typed target through `src/codegen/lower_inst/runtime_calls.rs`; PHP builtin names do not participate in dispatch.

## Semantic descriptor

- **Target strategy**: `runtime_call`
- **Validation**: `shared`
- **Result type source**: `declared`
- **Result ownership**: `non_heap`
- **Effects**: `static (0 declared effects)`
- **Requirements**: `static (1 requirements)`
- **Callable policy**: `static_only`
- **Target support**: `macos-aarch64`, `linux-aarch64`, `linux-x86_64`

## EIR and runtime boundary

- **Typed EIR target**: `runtime.pcntl.wifcontinued`
- **Backend boundary**: `src/codegen/lower_inst/runtime_calls.rs` resolves the typed target without PHP-name dispatch.

## Signature summary

```php
function pcntl_wifcontinued(int $status): bool
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/pcntl/pcntl_wifcontinued.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/pcntl/pcntl_wifcontinued.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `capability-dependent`.
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `pcntl_wifcontinued()`](../../../php/builtins/misc/pcntl_wifcontinued.md)
