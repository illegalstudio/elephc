---
title: "sodium_crypto_box_keypair() - internals"
description: "Compiler internals for sodium_crypto_box_keypair(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 1041
---

## `sodium_crypto_box_keypair()` - internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_surfaces.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_surfaces.rs)
- **Lowering**: [`src/sodium_prelude.rs`:91](https://github.com/illegalstudio/elephc/blob/main/src/sodium_prelude.rs#L91) (`sodium_crypto_box_keypair`)
- **Function symbol**: `sodium_crypto_box_keypair()`


### Lowering notes

- Implemented by the compiler-injected sodium prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function sodium_crypto_box_keypair(): string
```

## What the type checker enforces

- **Arity**: takes no arguments.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/string/sodium_crypto_box_keypair.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/sodium_crypto_box_keypair.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `dynamic-language-surface`.
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `sodium_crypto_box_keypair()`](../../../php/builtins/string/sodium_crypto_box_keypair.md)
