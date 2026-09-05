---
title: "pcntl_setqos_class() — internals"
description: "Compiler internals for pcntl_setqos_class(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 341
---

## `pcntl_setqos_class()` — internals

## Where it lives

- **Signature**: [`src/builtins/system/pcntl_setqos_class.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/system/pcntl_setqos_class.rs)
- **Lowering**: [`src/builtins/semantics.rs`:610](https://github.com/illegalstudio/elephc/blob/main/src/builtins/semantics.rs#L610) (`lower_registry_call`)
- **Function symbol**: `lower_registry_call()`


### Lowering notes

- Uses the `runtime_call` strategy from the single-source builtin descriptor.
- Emits the typed EIR target `runtime.pcntl.setqos_class` through `BuiltinLoweringContext`.
- The backend resolves that typed target through `src/codegen/lower_inst/runtime_calls.rs`; PHP builtin names do not participate in dispatch.

## Semantic descriptor

- **Target strategy**: `runtime_call`
- **Validation**: `checker_hook`
- **Result type source**: `checked`
- **Result ownership**: `non_heap`
- **Effects**: `static (4 declared effects)`
- **Requirements**: `static (1 requirements)`
- **Callable policy**: `static_only`
- **Target support**: `macos-aarch64`

## EIR and runtime boundary

- **Typed EIR target**: `runtime.pcntl.setqos_class`
- **Backend boundary**: `src/codegen/lower_inst/runtime_calls.rs` resolves the typed target without PHP-name dispatch.

## Signature summary

```php
function pcntl_setqos_class(mixed $qos_class): void
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/pcntl/pcntl_setqos_class.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/pcntl/pcntl_setqos_class.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `capability-dependent`.
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `pcntl_setqos_class()`](../../../php/builtins/misc/pcntl_setqos_class.md)
