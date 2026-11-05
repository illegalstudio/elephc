---
title: "sodium_crypto_secretstream_xchacha20poly1305_push() - internals"
description: "Compiler internals for sodium_crypto_secretstream_xchacha20poly1305_push(): lowering path, type checks, and runtime helpers."
---

## `sodium_crypto_secretstream_xchacha20poly1305_push()` - internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_sodium.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_sodium.rs)
- **Lowering**: [`src/sodium_prelude/mutators.rs`](https://github.com/illegalstudio/elephc/blob/main/src/sodium_prelude/mutators.rs) (`sodium_crypto_secretstream_xchacha20poly1305_push`)
- **Function symbol**: `sodium_crypto_secretstream_xchacha20poly1305_push()`


### Lowering notes

- Implemented by the compiler-injected sodium prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function sodium_crypto_secretstream_xchacha20poly1305_push(string &$state, string $message, string $additional_data = '', int $tag = 0): string
```

## What the type checker enforces

- **Arity**: takes 2–4 arguments (2 optional).
- **By-reference parameters**: `$state`.

## Eval interpreter (magician)

Eval resolves the same native prelude declaration the AOT program calls ([`src/sodium_prelude/mutators.rs`](https://github.com/illegalstudio/elephc/blob/main/src/sodium_prelude/mutators.rs)); there is no separate `eval_builtin!` binding, so named and dynamic calls share the compiled by-reference call machinery.

## Cross-references

- [User reference for `sodium_crypto_secretstream_xchacha20poly1305_push()`](../../../php/builtins/sodium/sodium_crypto_secretstream_xchacha20poly1305_push.md)
