---
title: "strstr() — internals"
description: "Compiler internals for strstr(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 473
---

## `strstr()` — internals

## Where it lives

- **Signature**: [`src/builtins/string/strstr.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/string/strstr.rs)
- **Lowering**: [`src/builtins/semantics.rs`:544](https://github.com/illegalstudio/elephc/blob/main/src/builtins/semantics.rs#L544) (`lower_registry_call`)
- **Function symbol**: `lower_registry_call()`


### Lowering notes

- Uses the `runtime_call` strategy from the single-source builtin descriptor.
- Emits the typed EIR target `runtime.strstr` through `BuiltinLoweringContext`.
- The backend resolves that typed target through `src/codegen/lower_inst/runtime_calls.rs`; PHP builtin names do not participate in dispatch.

## Semantic descriptor

- **Target strategy**: `runtime_call`
- **Validation**: `checker_hook`
- **Result type source**: `checked`
- **Result ownership**: `fresh`
- **Effects**: `static (0 declared effects)`
- **Requirements**: `static (0 requirements)`
- **Callable policy**: `static_only`
- **Target support**: `macos-aarch64`, `linux-aarch64`, `linux-x86_64`

## EIR and runtime boundary

- **Typed EIR target**: `runtime.strstr`
- **Backend boundary**: `src/codegen/lower_inst/runtime_calls.rs` resolves the typed target without PHP-name dispatch.

## Signature summary

```php
function strstr(string $haystack, string $needle, bool $before_needle = false): mixed
```

## What the type checker enforces

- **Arity**: takes 2–3 arguments (1 optional).

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/string/strstr.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/strstr.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `interpreter-specific-value-semantics`.
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `strstr()`](../../../php/builtins/string/strstr.md)
