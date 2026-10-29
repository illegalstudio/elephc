---
title: "__elephc_tokenizer_error_location() - internals"
description: "Compiler internals for __elephc_tokenizer_error_location(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 1211
---

## `__elephc_tokenizer_error_location()` - internals

## Where it lives

- **Signature**: [`src/builtins/system/__elephc_tokenizer_error_location.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/system/__elephc_tokenizer_error_location.rs)
- **Lowering**: [`src/builtins/semantics.rs`:706](https://github.com/illegalstudio/elephc/blob/main/src/builtins/semantics.rs#L706) (`lower_registry_call`)
- **Function symbol**: `lower_registry_call()`


### Lowering notes

- Uses the `runtime_call` strategy from the single-source builtin descriptor.
- Emits the typed EIR target `runtime.__elephc_tokenizer_error_location` through `BuiltinLoweringContext`.
- The backend resolves that typed target through `src/codegen/lower_inst/runtime_calls.rs`; PHP builtin names do not participate in dispatch.

## Semantic descriptor

- **Target strategy**: `runtime_call`
- **Validation**: `signature`
- **Result type source**: `declared`
- **Result ownership**: `non_heap`
- **Effects**: `static (2 declared effects)`
- **Requirements**: `static (0 requirements)`
- **Callable policy**: `static_only`
- **Target support**: `macos-aarch64`, `ios-arm64`, `ios-sim-arm64`, `linux-aarch64`, `linux-x86_64`

## EIR and runtime boundary

- **Typed EIR target**: `runtime.__elephc_tokenizer_error_location`
- **Backend boundary**: `src/codegen/lower_inst/runtime_calls.rs` resolves the typed target without PHP-name dispatch.

## Signature summary

```php
function __elephc_tokenizer_error_location(object $error, int $line): int
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/tokenizer/location.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/tokenizer/location.rs) (`eval_builtin!`)
- **Execution**: shared generated-runtime ABI (`RuntimeBuiltinId(88)`).
- **Dispatch hooks**: _none_ (shared runtime dispatch)

## Cross-references

- _No user-facing reference - this is a compiler internal helper._
