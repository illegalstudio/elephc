---
title: "quotemeta() — internals"
description: "Compiler internals for quotemeta(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 446
---

## `quotemeta()` — internals

## Where it lives

- **Signature**: [`src/builtins/string/quotemeta.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/string/quotemeta.rs)
- **Lowering**: [`src/builtins/semantics.rs`:544](https://github.com/illegalstudio/elephc/blob/main/src/builtins/semantics.rs#L544) (`lower_registry_call`)
- **Function symbol**: `lower_registry_call()`


### Lowering notes

- Uses the `runtime_call` strategy from the single-source builtin descriptor.
- Emits the typed EIR target `runtime.string.quote_meta` through `BuiltinLoweringContext`.
- The backend resolves that typed target through `src/codegen/lower_inst/runtime_calls.rs`; PHP builtin names do not participate in dispatch.

## Semantic descriptor

- **Target strategy**: `runtime_call`
- **Validation**: `signature`
- **Result type source**: `declared`
- **Result ownership**: `fresh`
- **Effects**: `static (0 declared effects)`
- **Requirements**: `static (0 requirements)`
- **Callable policy**: `dynamic`
- **Target support**: `macos-aarch64`, `linux-aarch64`, `linux-x86_64`

## EIR and runtime boundary

- **Typed EIR target**: `runtime.string.quote_meta`
- **Backend boundary**: `src/codegen/lower_inst/runtime_calls.rs` resolves the typed target without PHP-name dispatch.

## Signature summary

```php
function quotemeta(string $string): string
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/string/quotemeta.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/quotemeta.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `interpreter-specific-value-semantics`.
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `quotemeta()`](../../../php/builtins/string/quotemeta.md)
