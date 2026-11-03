---
title: "sodium_crypto_box_seal_open() - internals"
description: "Compiler internals for sodium_crypto_box_seal_open(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 1035
---

## `sodium_crypto_box_seal_open()` - internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_surfaces.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_surfaces.rs)
- **Lowering**: [`src/sodium_prelude.rs`:138](https://github.com/illegalstudio/elephc/blob/main/src/sodium_prelude.rs#L138) (`sodium_crypto_box_seal_open`)
- **Function symbol**: `sodium_crypto_box_seal_open()`


### Lowering notes

- Implemented by the compiler-injected sodium prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function sodium_crypto_box_seal_open(string $ciphertext, string $key_pair): mixed
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/string/sodium_crypto_box_seal_open.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/sodium_crypto_box_seal_open.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `dynamic-language-surface`.
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `sodium_crypto_box_seal_open()`](../../../php/builtins/string/sodium_crypto_box_seal_open.md)
