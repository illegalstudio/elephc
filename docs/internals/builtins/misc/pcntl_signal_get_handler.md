---
title: "pcntl_signal_get_handler() — internals"
description: "Compiler internals for pcntl_signal_get_handler(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 344
---

## `pcntl_signal_get_handler()` — internals

## Where it lives

- **Signature**: [`src/builtins/system/pcntl_signal_get_handler.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/system/pcntl_signal_get_handler.rs)
- **Lowering**: [`src/builtins/semantics.rs`:610](https://github.com/illegalstudio/elephc/blob/main/src/builtins/semantics.rs#L610) (`lower_registry_call`)
- **Function symbol**: `lower_registry_call()`


### Lowering notes

- Uses the `runtime_call` strategy from the single-source builtin descriptor.
- Emits the typed EIR target `runtime.pcntl.signal_get_handler` through `BuiltinLoweringContext`.
- The backend resolves that typed target through `src/codegen/lower_inst/runtime_calls.rs`; PHP builtin names do not participate in dispatch.

## Semantic descriptor

- **Target strategy**: `runtime_call`
- **Validation**: `shared`
- **Result type source**: `declared`
- **Result ownership**: `fresh`
- **Effects**: `static (4 declared effects)`
- **Requirements**: `static (1 requirements)`
- **Callable policy**: `static_only`
- **Target support**: `macos-aarch64`, `linux-aarch64`, `linux-x86_64`

## EIR and runtime boundary

- **Typed EIR target**: `runtime.pcntl.signal_get_handler`
- **Backend boundary**: `src/codegen/lower_inst/runtime_calls.rs` resolves the typed target without PHP-name dispatch.

## Signature summary

```php
function pcntl_signal_get_handler(int $signal): mixed
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/pcntl/pcntl_signal_get_handler.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/pcntl/pcntl_signal_get_handler.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `capability-dependent`.
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `pcntl_signal_get_handler()`](../../../php/builtins/misc/pcntl_signal_get_handler.md)
