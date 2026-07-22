---
title: "ob_get_level() — internals"
description: "Compiler internals for ob_get_level(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 199
---

## `ob_get_level()` — internals

## Where it lives

- **Signature**: [`src/builtins/io/ob_get_level.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/io/ob_get_level.rs)
- **Lowering**: [`src/builtins/semantics.rs`:423](https://github.com/illegalstudio/elephc/blob/main/src/builtins/semantics.rs#L423) (`lower_registry_call`)
- **Function symbol**: `lower_registry_call()`


### Lowering notes

- Uses the `runtime_call` strategy from the single-source builtin descriptor.
- Emits the typed EIR target `runtime.ob_get_level` through `BuiltinLoweringContext`.
- The backend resolves that typed target through `src/codegen/lower_inst/runtime_calls.rs`; PHP builtin names do not participate in dispatch.

## Semantic descriptor

- **Target strategy**: `runtime_call`
- **Validation**: `signature`
- **Result type source**: `declared`
- **Result ownership**: `may_alias_arguments`
- **Effects**: `static (16 declared effects)`
- **Requirements**: `static (0 requirements)`
- **Callable policy**: `static_only`
- **Target support**: `macos-aarch64`, `linux-aarch64`, `linux-x86_64`

## EIR and runtime boundary

- **Typed EIR target**: `runtime.ob_get_level`
- **Backend boundary**: `src/codegen/lower_inst/runtime_calls.rs` resolves the typed target without PHP-name dispatch.

## Signature summary

```php
function ob_get_level(): int
```

## What the type checker enforces

- **Arity**: takes no arguments.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/core/ob_get_level.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/core/ob_get_level.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `ob_get_level()`](../../../php/builtins/io/ob_get_level.md)
