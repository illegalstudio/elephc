---
title: "bccomp() — internals"
description: "Compiler internals for bccomp(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 270
---

## `bccomp()` — internals

## Where it lives

- **Signature**: [`src/builtins/math/bccomp.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/math/bccomp.rs)
- **Lowering**: [`src/builtins/semantics.rs`:576](https://github.com/illegalstudio/elephc/blob/main/src/builtins/semantics.rs#L576) (`lower_registry_call`)
- **Function symbol**: `lower_registry_call()`


### Lowering notes

- Uses the `runtime_call` strategy from the single-source builtin descriptor.
- Emits the typed EIR target `runtime.bccomp` through `BuiltinLoweringContext`.
- The backend resolves that typed target through `src/codegen/lower_inst/runtime_calls.rs`; PHP builtin names do not participate in dispatch.

## Semantic descriptor

- **Target strategy**: `runtime_call`
- **Validation**: `signature`
- **Result type source**: `declared`
- **Result ownership**: `non_heap`
- **Effects**: `static (2 declared effects)`
- **Requirements**: `static (1 requirements)`
- **Callable policy**: `static_only`
- **Target support**: `macos-aarch64`, `ios-arm64`, `ios-sim-arm64`, `linux-aarch64`, `linux-x86_64`

## EIR and runtime boundary

- **Typed EIR target**: `runtime.bccomp`
- **Backend boundary**: `src/codegen/lower_inst/runtime_calls.rs` resolves the typed target without PHP-name dispatch.

## Signature summary

```php
function bccomp(string $num1, string $num2, int $scale = null): int
```

## What the type checker enforces

- **Arity**: takes 2–3 arguments (1 optional).

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/math/bccomp.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/math/bccomp.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `capability-dependent`.
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `bccomp()`](../../../php/builtins/math/bccomp.md)
