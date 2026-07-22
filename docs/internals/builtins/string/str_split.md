---
title: "str_split() — internals"
description: "Compiler internals for str_split(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 408
---

## `str_split()` — internals

## Where it lives

- **Signature**: [`src/builtins/string/str_split.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/string/str_split.rs)
- **Lowering**: [`src/builtins/semantics.rs`:423](https://github.com/illegalstudio/elephc/blob/main/src/builtins/semantics.rs#L423) (`lower_registry_call`)
- **Function symbol**: `lower_registry_call()`


### Lowering notes

- Uses the `runtime_call` strategy from the single-source builtin descriptor.
- Emits the typed EIR target `runtime.str_split` through `BuiltinLoweringContext`.
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

- **Typed EIR target**: `runtime.str_split`
- **Backend boundary**: `src/codegen/lower_inst/runtime_calls.rs` resolves the typed target without PHP-name dispatch.

## Signature summary

```php
function str_split(string $string, int $length = 1): array
```

## What the type checker enforces

- **Arity**: takes 1–2 arguments (1 optional).

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/string/str_split.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/str_split.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `str_split()`](../../../php/builtins/string/str_split.md)
