---
title: "password_needs_rehash() - internals"
description: "Compiler internals for password_needs_rehash(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 1024
---

## `password_needs_rehash()` - internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/password_prelude.rs`:143](https://github.com/illegalstudio/elephc/blob/main/src/password_prelude.rs#L143) (`password_needs_rehash`)
- **Function symbol**: `password_needs_rehash()`


### Lowering notes

- Implemented by the compiler-injected password prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function password_needs_rehash(string $hash, mixed $algo, array $options = []): bool
```

## What the type checker enforces

- **Arity**: takes 2–3 arguments (1 optional).

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/string/password_needs_rehash.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/password_needs_rehash.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `dynamic-language-surface`.
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `password_needs_rehash()`](../../../php/builtins/string/password_needs_rehash.md)
