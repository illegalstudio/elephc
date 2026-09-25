---
title: "mb_convert_variables() - internals"
description: "Compiler internals for mb_convert_variables(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 833
---

## `mb_convert_variables()` - internals

## Where it lives

- **Signature**: [`src/builtins/string/mb_convert_variables.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/string/mb_convert_variables.rs)
- **Lowering**: [`src/builtins/semantics.rs`:680](https://github.com/illegalstudio/elephc/blob/main/src/builtins/semantics.rs#L680) (`lower_registry_call`)
- **Function symbol**: `lower_registry_call()`


### Lowering notes

- Uses the `runtime_call` strategy from the single-source builtin descriptor.
- Emits the typed EIR target `runtime.mb_convert_variables` through `BuiltinLoweringContext`.
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

- **Typed EIR target**: `runtime.mb_convert_variables`
- **Backend boundary**: `src/codegen/lower_inst/runtime_calls.rs` resolves the typed target without PHP-name dispatch.

## Signature summary

```php
function mb_convert_variables(string $to_encoding, array|string $from_encoding, mixed $var, ...$vars): string|false
```

## What the type checker enforces

- **Arity**: takes exactly 3 arguments.
- **By-reference parameters**: `$var`.
- **Variadic**: collects excess arguments into `$vars`.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/string/mb_convert_variables.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/mb_convert_variables.rs) (`eval_builtin!`)
- **Execution**: shared generated-runtime ABI (`RuntimeBuiltinId(87)`).
- **Dispatch hooks**: _none_ (shared runtime dispatch)
- **By-reference parameters**: `$var`.
- **Variadic**: collects excess arguments into `$vars`.

## Cross-references

- [User reference for `mb_convert_variables()`](../../../php/builtins/string/mb_convert_variables.md)
