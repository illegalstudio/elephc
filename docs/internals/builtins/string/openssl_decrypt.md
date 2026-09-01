---
title: "openssl_decrypt() — internals"
description: "Compiler internals for openssl_decrypt(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 446
---

## `openssl_decrypt()` — internals

## Where it lives

- **Signature**: [`src/builtins/string/openssl_decrypt.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/string/openssl_decrypt.rs)
- **Lowering**: [`src/builtins/semantics.rs`:576](https://github.com/illegalstudio/elephc/blob/main/src/builtins/semantics.rs#L576) (`lower_registry_call`)
- **Function symbol**: `lower_registry_call()`


### Lowering notes

- Uses the `runtime_call` strategy from the single-source builtin descriptor.
- Emits the typed EIR target `runtime.openssl_decrypt` through `BuiltinLoweringContext`.
- The backend resolves that typed target through `src/codegen/lower_inst/runtime_calls.rs`; PHP builtin names do not participate in dispatch.

## Semantic descriptor

- **Target strategy**: `runtime_call`
- **Validation**: `checker_hook`
- **Result type source**: `checked`
- **Result ownership**: `fresh`
- **Effects**: `static (16 declared effects)`
- **Requirements**: `static (1 requirements)`
- **Callable policy**: `static_only`
- **Target support**: `macos-aarch64`, `ios-arm64`, `ios-sim-arm64`, `linux-aarch64`, `linux-x86_64`

## EIR and runtime boundary

- **Typed EIR target**: `runtime.openssl_decrypt`
- **Backend boundary**: `src/codegen/lower_inst/runtime_calls.rs` resolves the typed target without PHP-name dispatch.

## Signature summary

```php
function openssl_decrypt(string $data, string $cipher_algo, string $passphrase, int $options = 0, string $iv = '', mixed $tag = null, string $aad = ''): mixed
```

## What the type checker enforces

- **Arity**: takes 3–7 arguments (4 optional).

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/string/openssl_decrypt.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/openssl_decrypt.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `capability-dependent`.
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `openssl_decrypt()`](../../../php/builtins/string/openssl_decrypt.md)
