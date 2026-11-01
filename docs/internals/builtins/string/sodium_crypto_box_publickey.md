---
title: "sodium_crypto_box_publickey() - internals"
description: "Compiler internals for sodium_crypto_box_publickey(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 1031
---

## `sodium_crypto_box_publickey()` - internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_surfaces.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_surfaces.rs)
- **Lowering**: [`src/sodium_prelude.rs`:101](https://github.com/illegalstudio/elephc/blob/main/src/sodium_prelude.rs#L101) (`sodium_crypto_box_publickey`)
- **Function symbol**: `sodium_crypto_box_publickey()`


### Lowering notes

- Implemented by the compiler-injected sodium prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function sodium_crypto_box_publickey(string $key_pair): string
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/string/sodium_crypto_box_publickey.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/sodium_crypto_box_publickey.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `dynamic-language-surface`.
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `sodium_crypto_box_publickey()`](../../../php/builtins/string/sodium_crypto_box_publickey.md)
