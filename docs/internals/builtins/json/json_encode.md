---
title: "json_encode() — internals"
description: "Compiler internals for json_encode(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 250
---

## `json_encode()` — internals

## Where it lives

- **Signature**: [`src/builtins/system/json_encode.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/system/json_encode.rs)
- **Lowering**: [`src/builtins/semantics.rs`:425](https://github.com/illegalstudio/elephc/blob/main/src/builtins/semantics.rs#L425) (`lower_registry_call`)
- **Function symbol**: `lower_registry_call()`


### Lowering notes

- Uses the `runtime_call` strategy from the single-source builtin descriptor.
- Emits the typed EIR target `runtime.json_encode` through `BuiltinLoweringContext`.
- The backend resolves that typed target through `src/codegen/lower_inst/runtime_calls.rs`; PHP builtin names do not participate in dispatch.

## Semantic descriptor

- **Target strategy**: `runtime_call`
- **Validation**: `checker_hook`
- **Result type source**: `shared`
- **Result ownership**: `may_alias_arguments`
- **Effects**: `static (16 declared effects)`
- **Requirements**: `static (0 requirements)`
- **Callable policy**: `static_only`
- **Target support**: `macos-aarch64`, `linux-aarch64`, `linux-x86_64`

## EIR and runtime boundary

- **Typed EIR target**: `runtime.json_encode`
- **Backend boundary**: `src/codegen/lower_inst/runtime_calls.rs` resolves the typed target without PHP-name dispatch.

## Signature summary

```php
function json_encode(mixed $value, int $flags = 0, int $depth = 512): string
```

## What the type checker enforces

- **Arity**: takes 1–3 arguments (2 optional).

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/json/json_encode.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/json/json_encode.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `json_encode()`](../../../php/builtins/json/json_encode.md)
