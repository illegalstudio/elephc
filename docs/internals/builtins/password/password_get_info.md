---
title: "password_get_info() - internals"
description: "Compiler internals for password_get_info(): lowering path, type checks, and runtime helpers."
---

## `password_get_info()` - internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_password.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_password.rs)
- **Lowering**: [`src/sodium_prelude/password.rs`](https://github.com/illegalstudio/elephc/blob/main/src/sodium_prelude/password.rs) (`password_get_info`)
- **Function symbol**: `password_get_info()`


### Lowering notes

- Implemented by the compiler-injected password prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function password_get_info(string $hash): array
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

_Not callable from eval'd code - the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `password_get_info()`](../../../php/builtins/password/password_get_info.md)
