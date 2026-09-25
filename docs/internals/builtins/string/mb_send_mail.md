---
title: "mb_send_mail() - internals"
description: "Compiler internals for mb_send_mail(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 869
---

## `mb_send_mail()` - internals

## Where it lives

- **Signature**: [`src/builtins/string/mb_send_mail.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/string/mb_send_mail.rs)
- **Lowering**: [`src/builtins/semantics.rs`:680](https://github.com/illegalstudio/elephc/blob/main/src/builtins/semantics.rs#L680) (`lower_registry_call`)
- **Function symbol**: `lower_registry_call()`


### Lowering notes

- Uses the `runtime_call` strategy from the single-source builtin descriptor.
- Emits the typed EIR target `runtime.mb_send_mail` through `BuiltinLoweringContext`.
- The backend resolves that typed target through `src/codegen/lower_inst/runtime_calls.rs`; PHP builtin names do not participate in dispatch.

## Semantic descriptor

- **Target strategy**: `runtime_call`
- **Validation**: `checker_hook`
- **Result type source**: `checked`
- **Result ownership**: `non_heap`
- **Effects**: `static (17 declared effects)`
- **Requirements**: `static (1 requirements)`
- **Callable policy**: `dynamic_target`
- **Target support**: `macos-aarch64`, `ios-arm64`, `ios-sim-arm64`, `linux-aarch64`, `linux-x86_64`

## EIR and runtime boundary

- **Typed EIR target**: `runtime.mb_send_mail`
- **Backend boundary**: `src/codegen/lower_inst/runtime_calls.rs` resolves the typed target without PHP-name dispatch.

## Signature summary

```php
function mb_send_mail(string $to, string $subject, string $message, array|string $additional_headers = [], ?string $additional_params = null): bool
```

## What the type checker enforces

- **Arity**: takes 3–5 arguments (2 optional).

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/string/mb_send_mail.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/mb_send_mail.rs) (`eval_builtin!`)
- **Execution**: shared generated-runtime ABI (`RuntimeBuiltinId(86)`).
- **Dispatch hooks**: _none_ (shared runtime dispatch)

## Cross-references

- [User reference for `mb_send_mail()`](../../../php/builtins/string/mb_send_mail.md)
