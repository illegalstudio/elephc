---
title: "sodium_crypto_secretstream_xchacha20poly1305_pull() - internals"
description: "Compiler internals for sodium_crypto_secretstream_xchacha20poly1305_pull(): lowering path, type checks, and runtime helpers."
---

## `sodium_crypto_secretstream_xchacha20poly1305_pull()` - internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_sodium.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_sodium.rs)
- **Lowering**: [`src/sodium_prelude/mutators.rs`](https://github.com/illegalstudio/elephc/blob/main/src/sodium_prelude/mutators.rs) (`sodium_crypto_secretstream_xchacha20poly1305_pull`)
- **Function symbol**: `sodium_crypto_secretstream_xchacha20poly1305_pull()`


### Lowering notes

- Implemented by the compiler-injected sodium prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function sodium_crypto_secretstream_xchacha20poly1305_pull(string &$state, string $ciphertext, string $additional_data = ''): array|false
```

## What the type checker enforces

- **Arity**: takes 2–3 arguments (1 optional).
- **By-reference parameters**: `$state`.

## Eval interpreter (magician)

Eval resolves the same native prelude declaration the AOT program calls ([`src/sodium_prelude/mutators.rs`](https://github.com/illegalstudio/elephc/blob/main/src/sodium_prelude/mutators.rs)); there is no separate `eval_builtin!` binding, so named and dynamic calls share the compiled by-reference call machinery.

## Cross-references

- [User reference for `sodium_crypto_secretstream_xchacha20poly1305_pull()`](../../../php/builtins/sodium/sodium_crypto_secretstream_xchacha20poly1305_pull.md)
