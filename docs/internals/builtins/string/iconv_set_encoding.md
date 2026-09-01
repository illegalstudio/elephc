---
title: "iconv_set_encoding() — internals"
description: "Compiler internals for iconv_set_encoding(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 428
---

## `iconv_set_encoding()` — internals

## Where it lives

- **Signature**: [`src/builtins/string/iconv_set_encoding.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/string/iconv_set_encoding.rs)
- **Lowering**: [`src/builtins/semantics.rs`:576](https://github.com/illegalstudio/elephc/blob/main/src/builtins/semantics.rs#L576) (`lower_registry_call`)
- **Function symbol**: `lower_registry_call()`


### Lowering notes

- Uses the `runtime_call` strategy from the single-source builtin descriptor.
- Emits the typed EIR target `runtime.iconv_set_encoding` through `BuiltinLoweringContext`.
- The backend resolves that typed target through `src/codegen/lower_inst/runtime_calls.rs`; PHP builtin names do not participate in dispatch.

## Semantic descriptor

- **Target strategy**: `runtime_call`
- **Validation**: `checker_hook`
- **Result type source**: `checked`
- **Result ownership**: `non_heap`
- **Effects**: `static (7 declared effects)`
- **Requirements**: `static (2 requirements)`
- **Callable policy**: `static_only`
- **Target support**: `macos-aarch64`, `ios-arm64`, `ios-sim-arm64`, `linux-aarch64`, `linux-x86_64`

## EIR and runtime boundary

- **Typed EIR target**: `runtime.iconv_set_encoding`
- **Backend boundary**: `src/codegen/lower_inst/runtime_calls.rs` resolves the typed target without PHP-name dispatch.

## Signature summary

```php
function iconv_set_encoding(string $type, string $encoding): bool
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/string/iconv_set_encoding.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/iconv_set_encoding.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `capability-dependent`.
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `iconv_set_encoding()`](../../../php/builtins/string/iconv_set_encoding.md)
