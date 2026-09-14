---
title: "mb_list_encodings() - internals"
description: "Compiler internals for mb_list_encodings(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 828
---

## `mb_list_encodings()` - internals

## Where it lives

- **Signature**: [`src/builtins/string/mb_list_encodings.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/string/mb_list_encodings.rs)
- **Lowering**: [`src/builtins/semantics.rs`:643](https://github.com/illegalstudio/elephc/blob/main/src/builtins/semantics.rs#L643) (`lower_registry_call`)
- **Function symbol**: `lower_registry_call()`


### Lowering notes

- Uses the `runtime_call` strategy from the single-source builtin descriptor.
- Emits the typed EIR target `runtime.mb_list_encodings` through `BuiltinLoweringContext`.
- The backend resolves that typed target through `src/codegen/lower_inst/runtime_calls.rs`; PHP builtin names do not participate in dispatch.

## Semantic descriptor

- **Target strategy**: `runtime_call`
- **Validation**: `checker_hook`
- **Result type source**: `checked`
- **Result ownership**: `fresh`
- **Effects**: `static (17 declared effects)`
- **Requirements**: `static (1 requirements)`
- **Callable policy**: `dynamic_target`
- **Target support**: `macos-aarch64`, `ios-arm64`, `ios-sim-arm64`, `linux-aarch64`, `linux-x86_64`

## EIR and runtime boundary

- **Typed EIR target**: `runtime.mb_list_encodings`
- **Backend boundary**: `src/codegen/lower_inst/runtime_calls.rs` resolves the typed target without PHP-name dispatch.

## Signature summary

```php
function mb_list_encodings(): array
```

## What the type checker enforces

- **Arity**: takes no arguments.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/string/mb_list_encodings.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/mb_list_encodings.rs) (`eval_builtin!`)
- **Execution**: shared generated-runtime ABI (`RuntimeBuiltinId(57)`).
- **Dispatch hooks**: _none_ (shared runtime dispatch)

## Cross-references

- [User reference for `mb_list_encodings()`](../../../php/builtins/string/mb_list_encodings.md)
