---
title: "hash_pbkdf2() - internals"
description: "Compiler internals for hash_pbkdf2(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 920
---

## `hash_pbkdf2()` - internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_surfaces.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_surfaces.rs)
- **Lowering**: [`src/hash_pbkdf2_prelude.rs`:178](https://github.com/illegalstudio/elephc/blob/main/src/hash_pbkdf2_prelude.rs#L178) (`hash_pbkdf2`)
- **Function symbol**: `hash_pbkdf2()`


### Lowering notes

- Implemented by the compiler-injected hash_pbkdf2 prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function hash_pbkdf2(string $algo, string $password, string $salt, int $iterations, int $length = 0, bool $binary = false, array $options = []): string
```

## What the type checker enforces

- **Arity**: takes 4–7 arguments (3 optional).

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/string/hash_pbkdf2.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/hash_pbkdf2.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `dynamic-language-surface`.
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `hash_pbkdf2()`](../../../php/builtins/string/hash_pbkdf2.md)
