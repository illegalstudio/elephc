---
title: "password_verify() - internals"
description: "Compiler internals for password_verify(): lowering path, type checks, and runtime helpers."
---

## `password_verify()` - internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_password.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_password.rs)
- **Lowering**: [`src/sodium_prelude/password.rs`](https://github.com/illegalstudio/elephc/blob/main/src/sodium_prelude/password.rs) (`password_verify`)
- **Function symbol**: `password_verify()`


### Lowering notes

- Implemented by the compiler-injected password prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function password_verify(string $password, string $hash): bool
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Eval interpreter (magician)

_Not callable from eval'd code - the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `password_verify()`](../../../php/builtins/password/password_verify.md)
