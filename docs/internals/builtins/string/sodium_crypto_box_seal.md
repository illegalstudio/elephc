---
title: "sodium_crypto_box_seal() - internals"
description: "Compiler internals for sodium_crypto_box_seal(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 1043
---

## `sodium_crypto_box_seal()` - internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_surfaces.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_surfaces.rs)
- **Lowering**: [`src/sodium_prelude.rs`:119](https://github.com/illegalstudio/elephc/blob/main/src/sodium_prelude.rs#L119) (`sodium_crypto_box_seal`)
- **Function symbol**: `sodium_crypto_box_seal()`


### Lowering notes

- Implemented by the compiler-injected sodium prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function sodium_crypto_box_seal(string $message, string $public_key): string
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/string/sodium_crypto_box_seal.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/sodium_crypto_box_seal.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `dynamic-language-surface`.
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `sodium_crypto_box_seal()`](../../../php/builtins/string/sodium_crypto_box_seal.md)
