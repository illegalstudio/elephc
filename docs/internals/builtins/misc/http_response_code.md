---
title: "http_response_code() — internals"
description: "Compiler internals for http_response_code(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 295
---

## `http_response_code()` — internals

## Where it lives

- **Signature**: [`src/builtins/system/http_response_code.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/system/http_response_code.rs)
- **Lowering**: [`src/builtins/semantics.rs`:423](https://github.com/illegalstudio/elephc/blob/main/src/builtins/semantics.rs#L423) (`lower_registry_call`)
- **Function symbol**: `lower_registry_call()`


### Lowering notes

- Uses the `runtime_call` strategy from the single-source builtin descriptor.
- Emits the typed EIR target `runtime.http_response_code` through `BuiltinLoweringContext`.
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

- **Typed EIR target**: `runtime.http_response_code`
- **Backend boundary**: `src/codegen/lower_inst/runtime_calls.rs` resolves the typed target without PHP-name dispatch.

## Signature summary

```php
function http_response_code(int $response_code = 0): int
```

## What the type checker enforces

- **Arity**: takes 0–1 arguments (1 optional).

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/time/http_response_code.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/time/http_response_code.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `http_response_code()`](../../../php/builtins/misc/http_response_code.md)
