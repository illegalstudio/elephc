---
title: "ob_end_flush() — internals"
description: "Compiler internals for ob_end_flush(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 218
---

## `ob_end_flush()` — internals

## Where it lives

- **Signature**: [`src/builtins/io/ob_end_flush.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/io/ob_end_flush.rs)
- **Lowering**: [`src/builtins/semantics.rs`:560](https://github.com/illegalstudio/elephc/blob/main/src/builtins/semantics.rs#L560) (`lower_registry_call`)
- **Function symbol**: `lower_registry_call()`


### Lowering notes

- Uses the `runtime_call` strategy from the single-source builtin descriptor.
- Emits the typed EIR target `runtime.ob_end_flush` through `BuiltinLoweringContext`.
- The backend resolves that typed target through `src/codegen/lower_inst/runtime_calls.rs`; PHP builtin names do not participate in dispatch.

## Semantic descriptor

- **Target strategy**: `runtime_call`
- **Validation**: `signature`
- **Result type source**: `declared`
- **Result ownership**: `may_alias_arguments`
- **Effects**: `static (16 declared effects)`
- **Requirements**: `static (0 requirements)`
- **Callable policy**: `static_only`
- **Target support**: `macos-aarch64`, `ios-arm64`, `ios-sim-arm64`, `linux-aarch64`, `linux-x86_64`

## EIR and runtime boundary

- **Typed EIR target**: `runtime.ob_end_flush`
- **Backend boundary**: `src/codegen/lower_inst/runtime_calls.rs` resolves the typed target without PHP-name dispatch.

## Signature summary

```php
function ob_end_flush(): bool
```

## What the type checker enforces

- **Arity**: takes no arguments.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/core/ob_end_flush.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/core/ob_end_flush.rs) (`eval_builtin!`)
- **Execution**: shared generated-runtime ABI (`RuntimeBuiltinId(21)`).
- **Dispatch hooks**: _none_ (shared runtime dispatch)

## Cross-references

- [User reference for `ob_end_flush()`](../../../php/builtins/io/ob_end_flush.md)
