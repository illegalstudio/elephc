---
title: "pcntl_daemon() — internals"
description: "Compiler internals for pcntl_daemon(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 329
---

## `pcntl_daemon()` — internals

## Where it lives

- **Signature**: [`src/builtins/system/pcntl_daemon.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/system/pcntl_daemon.rs)
- **Lowering**: [`src/builtins/semantics.rs`:610](https://github.com/illegalstudio/elephc/blob/main/src/builtins/semantics.rs#L610) (`lower_registry_call`)
- **Function symbol**: `lower_registry_call()`


### Lowering notes

- Uses the `runtime_call` strategy from the single-source builtin descriptor.
- Emits the typed EIR target `runtime.pcntl.daemon` through `BuiltinLoweringContext`.
- The backend resolves that typed target through `src/codegen/lower_inst/runtime_calls.rs`; PHP builtin names do not participate in dispatch.

## Semantic descriptor

- **Target strategy**: `runtime_call`
- **Validation**: `shared`
- **Result type source**: `declared`
- **Result ownership**: `non_heap`
- **Effects**: `static (6 declared effects)`
- **Requirements**: `static (1 requirements)`
- **Callable policy**: `static_only`
- **Target support**: `macos-aarch64`, `linux-aarch64`, `linux-x86_64`

## EIR and runtime boundary

- **Typed EIR target**: `runtime.pcntl.daemon`
- **Backend boundary**: `src/codegen/lower_inst/runtime_calls.rs` resolves the typed target without PHP-name dispatch.

## Signature summary

```php
function pcntl_daemon(bool $no_chdir = false, bool $no_close = false): bool
```

## What the type checker enforces

- **Arity**: takes 0–2 arguments (2 optional).

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/pcntl/pcntl_daemon.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/pcntl/pcntl_daemon.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `runtime-state-or-resource`.
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `pcntl_daemon()`](../../../php/builtins/misc/pcntl_daemon.md)
