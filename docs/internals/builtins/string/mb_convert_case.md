---
title: "mb_convert_case() — internals"
description: "Compiler internals for mb_convert_case(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 468
---

## `mb_convert_case()` — internals

## Where it lives

- **Signature**: [`src/builtins/string/mb_convert_case.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/string/mb_convert_case.rs)
- **Lowering**: [`src/builtins/semantics.rs`:553](https://github.com/illegalstudio/elephc/blob/main/src/builtins/semantics.rs#L553) (`lower_registry_call`)
- **Function symbol**: `lower_registry_call()`


### Lowering notes

- Uses the `runtime_call` strategy from the single-source builtin descriptor.
- Emits the typed EIR target `runtime.mb_convert_case` through `BuiltinLoweringContext`.
- The backend resolves that typed target through `src/codegen/lower_inst/runtime_calls.rs`; PHP builtin names do not participate in dispatch.

## Semantic descriptor

- **Target strategy**: `runtime_call`
- **Validation**: `checker_hook`
- **Result type source**: `checked`
- **Result ownership**: `fresh`
- **Effects**: `static (2 declared effects)`
- **Requirements**: `static (1 requirements)`
- **Callable policy**: `static_only`
- **Target support**: `macos-aarch64`, `ios-arm64`, `ios-sim-arm64`, `linux-aarch64`, `linux-x86_64`

## EIR and runtime boundary

- **Typed EIR target**: `runtime.mb_convert_case`
- **Backend boundary**: `src/codegen/lower_inst/runtime_calls.rs` resolves the typed target without PHP-name dispatch.

## Signature summary

```php
function mb_convert_case(string $string, int $mode, string $encoding = null): string
```

## What the type checker enforces

- **Arity**: takes 2–3 arguments (1 optional).

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/string/mb_convert_case.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/mb_convert_case.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `capability-dependent`.
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `mb_convert_case()`](../../../php/builtins/string/mb_convert_case.md)
