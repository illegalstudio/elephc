---
title: "headers_sent() - internals"
description: "Compiler internals for headers_sent(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 720
---

## `headers_sent()` - internals

## Where it lives

- **Signature**: [`src/builtins/system/headers_sent.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/system/headers_sent.rs)
- **Lowering**: [`src/builtins/semantics.rs`:752](https://github.com/illegalstudio/elephc/blob/main/src/builtins/semantics.rs#L752) (`lower_registry_call`)
- **Function symbol**: `lower_registry_call()`


### Lowering notes

- Uses the `runtime_call` strategy from the single-source builtin descriptor.
- Emits the typed EIR target `runtime.headers_sent` through `BuiltinLoweringContext`.
- The backend resolves that typed target through `src/codegen/lower_inst/runtime_calls.rs`; PHP builtin names do not participate in dispatch.

## Semantic descriptor

- **Target strategy**: `runtime_call`
- **Validation**: `checker_hook`
- **Result type source**: `checked`
- **Result ownership**: `may_alias_arguments`
- **Effects**: `static (16 declared effects)`
- **Requirements**: `static (0 requirements)`
- **Callable policy**: `static_only`
- **Target support**: `macos-aarch64`, `ios-arm64`, `ios-sim-arm64`, `linux-aarch64`, `linux-x86_64`

## EIR and runtime boundary

- **Typed EIR target**: `runtime.headers_sent`
- **Backend boundary**: `src/codegen/lower_inst/runtime_calls.rs` resolves the typed target without PHP-name dispatch.

## Signature summary

```php
function headers_sent(mixed $filename = null, mixed $line = null): bool
```

## What the type checker enforces

- **Arity**: takes 0–2 arguments (2 optional).
- **By-reference parameters**: `$filename`, `$line`.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/time/headers_sent.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/time/headers_sent.rs) (`eval_builtin!`)
- **Execution**: generated-runtime ABI (`RuntimeBuiltinId(96)`) with a Magician fallback adapter.
- **Adapter reason**: `additional-signature-semantics`.
- **Dispatch hooks**: `direct`, `values`
- **By-reference parameters**: `$filename`, `$line`.

## Cross-references

- [User reference for `headers_sent()`](../../../php/builtins/misc/headers_sent.md)
