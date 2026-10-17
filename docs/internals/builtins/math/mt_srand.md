---
title: "mt_srand() - internals"
description: "Compiler internals for mt_srand(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 593
---

## `mt_srand()` - internals

## Where it lives

- **Signature**: [`src/builtins/math/mt_srand.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/math/mt_srand.rs)
- **Lowering**: [`src/builtins/semantics.rs`:680](https://github.com/illegalstudio/elephc/blob/main/src/builtins/semantics.rs#L680) (`lower_registry_call`)
- **Function symbol**: `lower_registry_call()`


### Lowering notes

- Uses the `runtime_call` strategy from the single-source builtin descriptor.
- Emits the typed EIR target `runtime.mt_srand` through `BuiltinLoweringContext`.
- The backend resolves that typed target through `src/codegen/lower_inst/runtime_calls.rs`; PHP builtin names do not participate in dispatch.

## Semantic descriptor

- **Target strategy**: `runtime_call`
- **Validation**: `signature`
- **Result type source**: `declared`
- **Result ownership**: `may_alias_arguments`
- **Effects**: `static (4 declared effects)`
- **Requirements**: `static (0 requirements)`
- **Callable policy**: `static_only`
- **Target support**: `macos-aarch64`, `ios-arm64`, `ios-sim-arm64`, `linux-aarch64`, `linux-x86_64`

## EIR and runtime boundary

- **Typed EIR target**: `runtime.mt_srand`
- **Backend boundary**: `src/codegen/lower_inst/runtime_calls.rs` resolves the typed target without PHP-name dispatch.

## Signature summary

```php
function mt_srand(?int $seed = null, int $mode = MT_RAND_MT19937): void
```

## What the type checker enforces

- **Arity**: takes 0–2 arguments (2 optional).

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/math/mt_srand.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/math/mt_srand.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `interpreter-specific-value-semantics`.
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `mt_srand()`](../../../php/builtins/math/mt_srand.md)
