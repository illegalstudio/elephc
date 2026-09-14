---
title: "mb_encode_mimeheader() - internals"
description: "Compiler internals for mb_encode_mimeheader(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 807
---

## `mb_encode_mimeheader()` - internals

## Where it lives

- **Signature**: [`src/builtins/string/mb_encode_mimeheader.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/string/mb_encode_mimeheader.rs)
- **Lowering**: [`src/builtins/semantics.rs`:643](https://github.com/illegalstudio/elephc/blob/main/src/builtins/semantics.rs#L643) (`lower_registry_call`)
- **Function symbol**: `lower_registry_call()`


### Lowering notes

- Uses the `runtime_call` strategy from the single-source builtin descriptor.
- Emits the typed EIR target `runtime.mb_encode_mimeheader` through `BuiltinLoweringContext`.
- The backend resolves that typed target through `src/codegen/lower_inst/runtime_calls.rs`; PHP builtin names do not participate in dispatch.

## Semantic descriptor

- **Target strategy**: `runtime_call`
- **Validation**: `checker_hook`
- **Result type source**: `checked`
- **Result ownership**: `fresh`
- **Effects**: `static (17 declared effects)`
- **Requirements**: `static (1 requirements)`
- **Callable policy**: `dynamic_target`
- **Target support**: `macos-aarch64`, `ios-arm64`, `ios-sim-arm64`, `linux-aarch64`, `linux-x86_64`

## EIR and runtime boundary

- **Typed EIR target**: `runtime.mb_encode_mimeheader`
- **Backend boundary**: `src/codegen/lower_inst/runtime_calls.rs` resolves the typed target without PHP-name dispatch.

## Signature summary

```php
function mb_encode_mimeheader(string $string, ?string $charset = null, ?string $transfer_encoding = null, string $newline = "\r\n", int $indent = 0): string
```

## What the type checker enforces

- **Arity**: takes 1–5 arguments (4 optional).

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/string/mb_encode_mimeheader.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/mb_encode_mimeheader.rs) (`eval_builtin!`)
- **Execution**: shared generated-runtime ABI (`RuntimeBuiltinId(64)`).
- **Dispatch hooks**: _none_ (shared runtime dispatch)

## Cross-references

- [User reference for `mb_encode_mimeheader()`](../../../php/builtins/string/mb_encode_mimeheader.md)
