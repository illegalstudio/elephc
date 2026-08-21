---
title: "openssl_encrypt() — internals"
description: "Compiler internals for openssl_encrypt(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 442
---

## `openssl_encrypt()` — internals

## Where it lives

- **Signature**: [`src/builtins/string/openssl_encrypt.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/string/openssl_encrypt.rs)
- **Lowering**: [`src/builtins/semantics.rs`:551](https://github.com/illegalstudio/elephc/blob/main/src/builtins/semantics.rs#L551) (`lower_registry_call`)
- **Function symbol**: `lower_registry_call()`


### Lowering notes

- Uses the `runtime_call` strategy from the single-source builtin descriptor.
- Emits the typed EIR target `runtime.openssl_encrypt` through `BuiltinLoweringContext`.
- The backend resolves that typed target through `src/codegen/lower_inst/runtime_calls.rs`; PHP builtin names do not participate in dispatch.

## Semantic descriptor

- **Target strategy**: `runtime_call`
- **Validation**: `checker_hook`
- **Result type source**: `checked`
- **Result ownership**: `fresh`
- **Effects**: `static (16 declared effects)`
- **Requirements**: `static (1 requirements)`
- **Callable policy**: `static_only`
- **Target support**: `macos-aarch64`, `linux-aarch64`, `linux-x86_64`

## EIR and runtime boundary

- **Typed EIR target**: `runtime.openssl_encrypt`
- **Backend boundary**: `src/codegen/lower_inst/runtime_calls.rs` resolves the typed target without PHP-name dispatch.

## Signature summary

```php
function openssl_encrypt(string $data, string $cipher_algo, string $passphrase, int $options = 0, string $iv = '', mixed $tag = null, string $aad = '', int $tag_length = 16): mixed
```

## What the type checker enforces

- **Arity**: takes 3–8 arguments (5 optional).
- **By-reference parameters**: `$tag`.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/string/openssl_encrypt.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/openssl_encrypt.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `by-reference-or-lvalue`.
- **Dispatch hooks**: `direct`, `values`
- **By-reference parameters**: `$tag`.

## Cross-references

- [User reference for `openssl_encrypt()`](../../../php/builtins/string/openssl_encrypt.md)
