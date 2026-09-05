---
title: "setrawcookie() — internals"
description: "Compiler internals for setrawcookie(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 874
---

## `setrawcookie()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/web_prelude/build.rs`:1296](https://github.com/illegalstudio/elephc/blob/main/src/web_prelude/build.rs#L1296) (`setrawcookie`)
- **Function symbol**: `setrawcookie()`


### Lowering notes

- Implemented by the compiler-injected web prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function setrawcookie(mixed $name, mixed $value = '', mixed $expires = 0, mixed $path = '', mixed $domain = '', mixed $secure = false, mixed $httponly = false): mixed
```

## What the type checker enforces

- **Arity**: takes 1–7 arguments (6 optional).

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `setrawcookie()`](../../../php/builtins/web/setrawcookie.md)
