---
title: "pcntl_setns() — internals"
description: "Compiler internals for pcntl_setns(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 338
---

## `pcntl_setns()` — internals

## Where it lives

- **Signature**: [`src/builtins/system/pcntl_setns.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/system/pcntl_setns.rs)
- **Lowering**: [`src/builtins/semantics.rs`:611](https://github.com/illegalstudio/elephc/blob/main/src/builtins/semantics.rs#L611) (`lower_registry_call`)
- **Function symbol**: `lower_registry_call()`


### Lowering notes

- Uses the `runtime_call` strategy from the single-source builtin descriptor.
- Emits the typed EIR target `runtime.pcntl.setns` through `BuiltinLoweringContext`.
- The backend resolves that typed target through `src/codegen/lower_inst/runtime_calls.rs`; PHP builtin names do not participate in dispatch.

## Semantic descriptor

- **Target strategy**: `runtime_call`
- **Validation**: `shared`
- **Result type source**: `declared`
- **Result ownership**: `non_heap`
- **Effects**: `static (6 declared effects)`
- **Requirements**: `static (1 requirements)`
- **Callable policy**: `static_only`
- **Target support**: `linux-aarch64`, `linux-x86_64`

## EIR and runtime boundary

- **Typed EIR target**: `runtime.pcntl.setns`
- **Backend boundary**: `src/codegen/lower_inst/runtime_calls.rs` resolves the typed target without PHP-name dispatch.

## Signature summary

```php
function pcntl_setns(int $process_id = null, int $nstype = 1073741824): bool
```

## What the type checker enforces

- **Arity**: takes 0–2 arguments (2 optional).

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/pcntl/pcntl_setns.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/pcntl/pcntl_setns.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `capability-dependent`.
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `pcntl_setns()`](../../../php/builtins/misc/pcntl_setns.md)
