---
title: "hash_hmac() — internals"
description: "Compiler internals for hash_hmac(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 361
---

## `hash_hmac()` — internals

## Where it lives

- **Signature**: [`src/builtins/string/hash_hmac.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/string/hash_hmac.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/strings.rs`:246](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/strings.rs#L246) (`lower_hash_hmac`)
- **Function symbol**: `lower_hash_hmac()`


### Lowering notes

- Lowers `hash_hmac(algo, data, key, binary?)` through the shared HMAC runtime dispatcher.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_hash_equals`
- `__rt_hash_hmac`

## Signature summary

```php
function hash_hmac(string $algo, string $data, string $key, bool $binary = false): string
```

## What the type checker enforces

- **Arity**: takes 3–4 arguments (1 optional).

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/string/hash_hmac.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/hash_hmac.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `hash_hmac()`](../../../php/builtins/string/hash_hmac.md)
