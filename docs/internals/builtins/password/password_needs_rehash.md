---
title: "password_needs_rehash() - internals"
description: "Compiler internals for password_needs_rehash(): lowering path, type checks, and runtime helpers."
---

## `password_needs_rehash()` - internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_password.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_password.rs)
- **Lowering**: [`src/sodium_prelude/password.rs`](https://github.com/illegalstudio/elephc/blob/main/src/sodium_prelude/password.rs) (`password_needs_rehash`)
- **Function symbol**: `password_needs_rehash()`


### Lowering notes

- Implemented by the compiler-injected password prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function password_needs_rehash(string $hash, string|int|null $algo, array $options = []): bool
```

## What the type checker enforces

- **Arity**: takes 2–3 arguments (1 optional).

## Eval interpreter (magician)

_Not callable from eval'd code - the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `password_needs_rehash()`](../../../php/builtins/password/password_needs_rehash.md)
