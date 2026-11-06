---
title: "password_algos() - internals"
description: "Compiler internals for password_algos(): lowering path, type checks, and runtime helpers."
---

## `password_algos()` - internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_password.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_password.rs)
- **Lowering**: [`src/sodium_prelude/password.rs`](https://github.com/illegalstudio/elephc/blob/main/src/sodium_prelude/password.rs) (`password_algos`)
- **Function symbol**: `password_algos()`


### Lowering notes

- Implemented by the compiler-injected password prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function password_algos(): array
```

## What the type checker enforces

- **Arity**: takes no arguments.

## Eval interpreter (magician)

_Not callable from eval'd code - the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `password_algos()`](../../../php/builtins/password/password_algos.md)
