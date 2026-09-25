---
title: "sapi_windows_generate_ctrl_event() - internals"
description: "Compiler internals for sapi_windows_generate_ctrl_event(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 696
---

## `sapi_windows_generate_ctrl_event()` - internals

## Where it lives

- **Signature**: [`src/builtins/system/sapi_windows/generate_ctrl_event.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/system/sapi_windows/generate_ctrl_event.rs)
- **Lowering**: [`src/builtins/semantics.rs`:695](https://github.com/illegalstudio/elephc/blob/main/src/builtins/semantics.rs#L695) (`lower_registry_call`)
- **Function symbol**: `lower_registry_call()`


### Lowering notes

- Uses the `runtime_call` strategy from the single-source builtin descriptor.
- Emits the typed EIR target `runtime.sapi_windows_generate_ctrl_event` through `BuiltinLoweringContext`.
- The backend resolves that typed target through `src/codegen/lower_inst/runtime_calls.rs`; PHP builtin names do not participate in dispatch.

## Semantic descriptor

- **Target strategy**: `runtime_call`
- **Validation**: `signature`
- **Result type source**: `declared`
- **Result ownership**: `may_alias_arguments`
- **Effects**: `static (16 declared effects)`
- **Requirements**: `static (0 requirements)`
- **Callable policy**: `static_only`
- **Target support**: `windows-x86_64`

## EIR and runtime boundary

- **Typed EIR target**: `runtime.sapi_windows_generate_ctrl_event`
- **Backend boundary**: `src/codegen/lower_inst/runtime_calls.rs` resolves the typed target without PHP-name dispatch.

## Signature summary

```php
function sapi_windows_generate_ctrl_event(int $event, int $pid = 0): bool
```

## What the type checker enforces

- **Arity**: takes 1–2 arguments (1 optional).

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/network_env/sapi_windows.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/network_env/sapi_windows.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `runtime-state-or-resource`.
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `sapi_windows_generate_ctrl_event()`](../../../php/builtins/misc/sapi_windows_generate_ctrl_event.md)
