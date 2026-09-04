---
title: "iconv_strrpos() — internals"
description: "Compiler internals for iconv_strrpos(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 486
---

## `iconv_strrpos()` — internals

## Where it lives

- **Signature**: [`src/builtins/string/iconv_strrpos.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/string/iconv_strrpos.rs)
- **Lowering**: [`src/builtins/semantics.rs`:560](https://github.com/illegalstudio/elephc/blob/main/src/builtins/semantics.rs#L560) (`lower_registry_call`)
- **Function symbol**: `lower_registry_call()`


### Lowering notes

- Uses the `runtime_call` strategy from the single-source builtin descriptor.
- Emits the typed EIR target `runtime.iconv_strrpos` through `BuiltinLoweringContext`.
- The backend resolves that typed target through `src/codegen/lower_inst/runtime_calls.rs`; PHP builtin names do not participate in dispatch.

## Semantic descriptor

- **Target strategy**: `runtime_call`
- **Validation**: `checker_hook`
- **Result type source**: `checked`
- **Result ownership**: `fresh`
- **Effects**: `static (7 declared effects)`
- **Requirements**: `static (2 requirements)`
- **Callable policy**: `static_only`
- **Target support**: `macos-aarch64`, `ios-arm64`, `ios-sim-arm64`, `linux-aarch64`, `linux-x86_64`

## EIR and runtime boundary

- **Typed EIR target**: `runtime.iconv_strrpos`
- **Backend boundary**: `src/codegen/lower_inst/runtime_calls.rs` resolves the typed target without PHP-name dispatch.

## Signature summary

```php
function iconv_strrpos(string $haystack, string $needle, string $encoding = null): mixed
```

## What the type checker enforces

- **Arity**: takes 2–3 arguments (1 optional).

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/string/iconv_strrpos.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/iconv_strrpos.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `capability-dependent`.
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `iconv_strrpos()`](../../../php/builtins/string/iconv_strrpos.md)
