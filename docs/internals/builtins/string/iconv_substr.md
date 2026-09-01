---
title: "iconv_substr() — internals"
description: "Compiler internals for iconv_substr(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 432
---

## `iconv_substr()` — internals

## Where it lives

- **Signature**: [`src/builtins/string/iconv_substr.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/string/iconv_substr.rs)
- **Lowering**: [`src/builtins/semantics.rs`:576](https://github.com/illegalstudio/elephc/blob/main/src/builtins/semantics.rs#L576) (`lower_registry_call`)
- **Function symbol**: `lower_registry_call()`


### Lowering notes

- Uses the `runtime_call` strategy from the single-source builtin descriptor.
- Emits the typed EIR target `runtime.iconv_substr` through `BuiltinLoweringContext`.
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

- **Typed EIR target**: `runtime.iconv_substr`
- **Backend boundary**: `src/codegen/lower_inst/runtime_calls.rs` resolves the typed target without PHP-name dispatch.

## Signature summary

```php
function iconv_substr(string $string, int $offset, int $length = null, string $encoding = null): mixed
```

## What the type checker enforces

- **Arity**: takes 2–4 arguments (2 optional).

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/string/iconv_substr.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/iconv_substr.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `capability-dependent`.
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `iconv_substr()`](../../../php/builtins/string/iconv_substr.md)
