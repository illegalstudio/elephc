---
title: "strcspn() - internals"
description: "Compiler internals for strcspn(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 927
---

## `strcspn()` - internals

## Where it lives

- **Signature**: [`src/builtins/string/strcspn.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/string/strcspn.rs)
- **Lowering**: [`src/builtins/semantics.rs`:704](https://github.com/illegalstudio/elephc/blob/main/src/builtins/semantics.rs#L704) (`lower_registry_call`)
- **Function symbol**: `lower_registry_call()`


### Lowering notes

- Uses the `runtime_call` strategy from the single-source builtin descriptor.
- Emits the typed EIR target `runtime.strcspn` through `BuiltinLoweringContext`.
- The backend resolves that typed target through `src/codegen/lower_inst/runtime_calls.rs`; PHP builtin names do not participate in dispatch.

## Semantic descriptor

- **Target strategy**: `runtime_call`
- **Validation**: `signature`
- **Result type source**: `declared`
- **Result ownership**: `may_alias_arguments`
- **Effects**: `static (0 declared effects)`
- **Requirements**: `static (0 requirements)`
- **Callable policy**: `static_only`
- **Target support**: `macos-aarch64`, `ios-arm64`, `ios-sim-arm64`, `linux-aarch64`, `linux-x86_64`

## EIR and runtime boundary

- **Typed EIR target**: `runtime.strcspn`
- **Backend boundary**: `src/codegen/lower_inst/runtime_calls.rs` resolves the typed target without PHP-name dispatch.

## Signature summary

```php
function strcspn(string $string, string $characters, int $offset = 0, ?int $length = null): int
```

## What the type checker enforces

- **Arity**: takes 2–4 arguments (2 optional).

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/string/strcspn.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/strcspn.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `interpreter-specific-value-semantics`.
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `strcspn()`](../../../php/builtins/string/strcspn.md)
